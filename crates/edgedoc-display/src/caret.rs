//! Map a page point to a source caret, and a source caret back to a page point.
//!
//! The layout expands merge fields, so a painted byte is not a source byte. [`SourceSpan`] is the
//! map. A field is atomic: a click selects it, and a caret never sits inside the marker.

use edgedoc_flow::{
    expanded_offset, source_offset, span_is_atomic, Fragment, PageGeometry, ParagraphFragment,
    SourceSpan,
};

/// A caret or a selection edge in one paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct CaretPoint {
    pub path: String,
    pub offset: usize,
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub h: f32,
    /// When a click lands on a field, the other edge of that field.
    pub select_end: Option<usize>,
}

/// One selection rectangle in page coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkBox {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The nearest caret to a page point.
pub fn hit_caret(pages: &[PageGeometry], page: usize, x: f32, y: f32) -> Option<CaretPoint> {
    let page_geo = pages.get(page)?;
    let mut best: Option<Candidate> = None;
    each_paragraph(&page_geo.fragments, &mut |para| {
        consider(para, page as u32, x, y, &mut best);
    });
    best.map(|candidate| candidate.into_point())
}

/// Where a source offset paints. A paragraph that breaks across pages returns the fragment that
/// holds the offset.
pub fn caret_point(pages: &[PageGeometry], path: &str, offset: usize) -> Option<CaretPoint> {
    let mut found = None;
    for (index, page) in pages.iter().enumerate() {
        each_paragraph(&page.fragments, &mut |para| {
            if para.path != path || found.is_some() {
                return;
            }
            if let Some(point) = point_in_fragment(para, index as u32, offset) {
                found = Some(point);
            }
        });
        if found.is_some() {
            break;
        }
    }
    found.or_else(|| fallback_point(pages, path, offset))
}

/// Rectangles that cover a source range. The ends must name paragraphs the layout painted.
pub fn selection_marks(
    pages: &[PageGeometry],
    start_path: &str,
    start: usize,
    end_path: &str,
    end: usize,
) -> Vec<MarkBox> {
    let mut marks = Vec::new();
    if start_path == end_path {
        let (from, to) = ordered(start, end);
        push_range(pages, start_path, from, to, &mut marks);
        return marks;
    }
    // A selection that leaves one paragraph paints each paragraph it covers in full, between the
    // two ends, in reading order — whichever end the person dragged from. Paths the layout did not
    // paint still paint their own ends.
    let order = paragraph_order(pages);
    let position = |path: &str| order.iter().position(|candidate| candidate == path);
    let (Some(a), Some(b)) = (position(start_path), position(end_path)) else {
        push_range(pages, start_path, start, usize::MAX, &mut marks);
        push_range(pages, end_path, 0, end, &mut marks);
        return marks;
    };
    let (first, first_offset, last, last_offset, from, to) = if a <= b {
        (start_path, start, end_path, end, a, b)
    } else {
        (end_path, end, start_path, start, b, a)
    };
    push_range(pages, first, first_offset, usize::MAX, &mut marks);
    for path in &order[from + 1..to] {
        push_range(pages, path, 0, usize::MAX, &mut marks);
    }
    push_range(pages, last, 0, last_offset, &mut marks);
    marks
}

/// The painted paragraphs in reading order, each once (a paragraph that breaks across pages is one).
pub fn paragraph_order(pages: &[PageGeometry]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for page in pages {
        each_paragraph(&page.fragments, &mut |para| {
            if seen.insert(para.path.clone()) {
                order.push(para.path.clone());
            }
        });
    }
    order
}

fn push_range(
    pages: &[PageGeometry],
    path: &str,
    from: usize,
    to: usize,
    marks: &mut Vec<MarkBox>,
) {
    for (index, page) in pages.iter().enumerate() {
        each_paragraph(&page.fragments, &mut |para| {
            if para.path != path {
                return;
            }
            for line_index in para.l0..para.l1 {
                let Some(line) = para.layout.lines.get(line_index) else {
                    continue;
                };
                let line_from = to_expanded(&para.source, from, para.text.len()).max(line.start);
                let end = if to == usize::MAX {
                    para.text.len()
                } else {
                    to_expanded(&para.source, to, para.text.len())
                };
                let line_to = end.min(line.stop);
                if line_to <= line.start || line_from >= line.stop || line_to <= line_from {
                    continue;
                }
                let x0 = boundary_x(para, line, line_from);
                let x1 = boundary_x(para, line, line_to);
                let (top, height) = line_top(para, line);
                let left = x0.min(x1);
                let width = (x0 - x1).abs().max(1.0);
                marks.push(MarkBox {
                    page: index as u32,
                    x: left,
                    y: top,
                    w: width,
                    h: height.max(1.0),
                });
            }
        });
    }
}

fn consider(para: &ParagraphFragment, page: u32, x: f32, y: f32, best: &mut Option<Candidate>) {
    if para.layout.lines.is_empty() {
        offer(
            best,
            Candidate {
                path: para.path.clone(),
                offset: 0,
                page,
                x: para.rect.x,
                y: para.rect.y,
                h: para.rect.h.max(14.0),
                select_end: None,
                distance: (y - para.rect.y).abs() + (x - para.rect.x).abs() * 0.01,
            },
        );
        return;
    }
    for line_index in para.l0..para.l1 {
        let Some(line) = para.layout.lines.get(line_index) else {
            continue;
        };
        let (top, height) = line_top(para, line);
        let mut vertical = 0.0;
        if y < top {
            vertical = top - y;
        } else if y > top + height {
            vertical = y - (top + height);
        }
        let local = x - para.rect.x;
        let count = line.c1.saturating_sub(line.c0);
        let mut boundary = count;
        let mut best_d = f32::MAX;
        for slot in 0..=count {
            let px = line.xs.get(slot).copied().unwrap_or(0.0);
            let distance = (local - px).abs();
            if distance < best_d {
                best_d = distance;
                boundary = slot;
            }
        }
        let expanded = expanded_at(para, line, boundary);
        let offset = to_source(&para.source, expanded);
        let select_end = atomic_end(&para.source, expanded);
        let caret_x = para.rect.x + line.xs.get(boundary).copied().unwrap_or(0.0);
        offer(
            best,
            Candidate {
                path: para.path.clone(),
                offset,
                page,
                x: caret_x,
                y: top,
                h: height.max(8.0),
                select_end,
                distance: vertical * 1000.0 + best_d,
            },
        );
    }
}

fn point_in_fragment(para: &ParagraphFragment, page: u32, offset: usize) -> Option<CaretPoint> {
    if para.layout.lines.is_empty() {
        if offset == 0 {
            return Some(CaretPoint {
                path: para.path.clone(),
                offset: 0,
                page,
                x: para.rect.x,
                y: para.rect.y,
                h: para.rect.h.max(14.0),
                select_end: None,
            });
        }
        return None;
    }
    let expanded = to_expanded(&para.source, offset, para.text.len());
    for line_index in para.l0..para.l1 {
        let line = para.layout.lines.get(line_index)?;
        let holds = (expanded >= line.start && expanded < line.stop)
            || (expanded == line.stop && line_index + 1 == para.l1);
        if !holds {
            continue;
        }
        let (top, height) = line_top(para, line);
        return Some(CaretPoint {
            path: para.path.clone(),
            offset,
            page,
            x: boundary_x(para, line, expanded),
            y: top,
            h: height.max(8.0),
            select_end: None,
        });
    }
    None
}

fn fallback_point(pages: &[PageGeometry], path: &str, offset: usize) -> Option<CaretPoint> {
    for (index, page) in pages.iter().enumerate() {
        let mut found = None;
        each_paragraph(&page.fragments, &mut |para| {
            if para.path == path {
                found = Some(CaretPoint {
                    path: path.to_string(),
                    offset,
                    page: index as u32,
                    x: para.rect.x,
                    y: para.rect.y,
                    h: 14.0,
                    select_end: None,
                });
            }
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

fn each_paragraph(fragments: &[Fragment], visit: &mut dyn FnMut(&ParagraphFragment)) {
    for fragment in fragments {
        match fragment {
            Fragment::Paragraph(para) => visit(para),
            Fragment::Table(table) => each_paragraph(&table.content, visit),
            Fragment::Picture { .. } | Fragment::Placeholder { .. } | Fragment::Rule { .. } => {}
        }
    }
}

fn line_top(para: &ParagraphFragment, line: &edgedoc_text::Line) -> (f32, f32) {
    let origin = para
        .layout
        .lines
        .get(para.l0)
        .map(|first| first.top)
        .unwrap_or(0.0);
    let top = para.rect.y + line.top - origin;
    let height = if line.height > 0.5 {
        line.height
    } else {
        (line.text_asc + line.text_desc).max(12.0)
    };
    (top, height)
}

fn expanded_at(para: &ParagraphFragment, line: &edgedoc_text::Line, boundary: usize) -> usize {
    let clusters = &para.layout.clusters;
    if boundary == 0 {
        return clusters.get(line.c0).map(|c| c.start).unwrap_or(line.start);
    }
    let count = line.c1.saturating_sub(line.c0);
    if boundary >= count {
        return clusters
            .get(line.c1.saturating_sub(1))
            .map(|c| c.end)
            .unwrap_or(line.stop);
    }
    clusters
        .get(line.c0 + boundary)
        .map(|c| c.start)
        .unwrap_or(line.start)
}

fn boundary_x(para: &ParagraphFragment, line: &edgedoc_text::Line, expanded: usize) -> f32 {
    let mut at = line.xs.len().saturating_sub(1);
    for (slot, cluster) in para
        .layout
        .clusters
        .iter()
        .take(line.c1)
        .skip(line.c0)
        .enumerate()
    {
        if expanded <= cluster.start {
            at = slot;
            break;
        }
        if expanded < cluster.end {
            let before = expanded - cluster.start;
            let after = cluster.end - expanded;
            at = if before <= after { slot } else { slot + 1 };
            break;
        }
    }
    para.rect.x + line.xs.get(at).copied().unwrap_or(0.0)
}

fn to_source(spans: &[SourceSpan], expanded: usize) -> usize {
    if spans.is_empty() {
        return expanded;
    }
    source_offset(spans, expanded).unwrap_or(expanded)
}

fn to_expanded(spans: &[SourceSpan], source: usize, len: usize) -> usize {
    if spans.is_empty() {
        return source.min(len);
    }
    expanded_offset(spans, source)
}

fn atomic_end(spans: &[SourceSpan], expanded: usize) -> Option<usize> {
    if !span_is_atomic(spans, expanded) {
        return None;
    }
    let source = source_offset(spans, expanded)?;
    Some(source.saturating_add(OBJ_LEN))
}

fn offer(best: &mut Option<Candidate>, candidate: Candidate) {
    let replace = match best {
        None => true,
        Some(current) => candidate.distance < current.distance,
    };
    if replace {
        *best = Some(candidate);
    }
}

fn ordered(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

struct Candidate {
    path: String,
    offset: usize,
    page: u32,
    x: f32,
    y: f32,
    h: f32,
    select_end: Option<usize>,
    distance: f32,
}

impl Candidate {
    fn into_point(self) -> CaretPoint {
        CaretPoint {
            path: self.path,
            offset: self.offset,
            page: self.page,
            x: self.x,
            y: self.y,
            h: self.h,
            select_end: self.select_end,
        }
    }
}

/// A Word object marker is the three-byte character U+FFFC.
const OBJ_LEN: usize = 3;

/// A line the arrow keys can land on.
#[derive(Clone)]
pub struct LineSlot {
    pub path: String,
    pub page: u32,
    pub top: f32,
    pub height: f32,
    pub expanded_start: usize,
    pub expanded_stop: usize,
    pub line: usize,
}

pub fn line_slots(pages: &[PageGeometry]) -> Vec<LineSlot> {
    let mut slots = Vec::new();
    for (page_index, page) in pages.iter().enumerate() {
        collect_slots(&page.fragments, page_index as u32, &mut slots);
    }
    slots
}

fn collect_slots(fragments: &[Fragment], page: u32, slots: &mut Vec<LineSlot>) {
    for fragment in fragments {
        match fragment {
            Fragment::Paragraph(para) => {
                if para.layout.lines.is_empty() {
                    slots.push(LineSlot {
                        path: para.path.clone(),
                        page,
                        top: para.rect.y,
                        height: para.rect.h.max(14.0),
                        expanded_start: 0,
                        expanded_stop: 0,
                        line: 0,
                    });
                    continue;
                }
                for line_index in para.l0..para.l1 {
                    let Some(line) = para.layout.lines.get(line_index) else {
                        continue;
                    };
                    let (top, height) = line_top(para, line);
                    slots.push(LineSlot {
                        path: para.path.clone(),
                        page,
                        top,
                        height,
                        expanded_start: line.start,
                        expanded_stop: line.stop,
                        line: line_index,
                    });
                }
            }
            Fragment::Table(table) => collect_slots(&table.content, page, slots),
            Fragment::Picture { .. } | Fragment::Placeholder { .. } | Fragment::Rule { .. } => {}
        }
    }
}

/// The source offset at `x` on a line the arrow keys already chose.
pub fn slot_offset(pages: &[PageGeometry], slot: &LineSlot, x: f32) -> usize {
    let mut offset = 0usize;
    if let Some(page) = pages.get(slot.page as usize) {
        each_paragraph(&page.fragments, &mut |para| {
            if para.path != slot.path || slot.line < para.l0 || slot.line >= para.l1 {
                return;
            }
            let Some(line) = para.layout.lines.get(slot.line) else {
                return;
            };
            let local = x - para.rect.x;
            let count = line.c1.saturating_sub(line.c0);
            let mut boundary = count;
            let mut best = f32::MAX;
            for index in 0..=count {
                let px = line.xs.get(index).copied().unwrap_or(0.0);
                let distance = (local - px).abs();
                if distance < best {
                    best = distance;
                    boundary = index;
                }
            }
            offset = to_source(&para.source, expanded_at(para, line, boundary));
        });
    }
    offset
}

/// The source offset at the start or the end of a line.
pub fn slot_edge(pages: &[PageGeometry], slot: &LineSlot, start: bool) -> usize {
    let expanded = if start {
        slot.expanded_start
    } else {
        slot.expanded_stop
    };
    let mut offset = expanded;
    if let Some(page) = pages.get(slot.page as usize) {
        each_paragraph(&page.fragments, &mut |para| {
            if para.path == slot.path && slot.line >= para.l0 && slot.line < para.l1 {
                offset = to_source(&para.source, expanded);
            }
        });
    }
    offset
}
