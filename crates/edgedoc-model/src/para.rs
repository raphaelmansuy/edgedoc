use std::sync::atomic::{AtomicU64, Ordering};

use edgedoc_unit::{Emu, IntoTwips};

use crate::error::{ModelError, ModelResult};
use crate::fields::Field;
use crate::props::{CharProps, ListKind, ListSpec, ParaProps, Retention, TabStop};
use crate::table::Borders;

pub const OBJ: char = '\u{FFFC}';
pub const PAGE_BREAK: char = '\u{000C}';
/// A column break, serialized as `w:br w:type="column"`.
pub const COLUMN_BREAK: char = '\u{000E}';

static REV: AtomicU64 = AtomicU64::new(1);

fn next_rev() -> u64 {
    REV.fetch_add(1, Ordering::Relaxed)
}

/// Where a floating drawing sits relative to the text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Anchor {
    pub behind_text: bool,
    pub h_anchor: Option<String>,
    pub v_anchor: Option<String>,
    pub h_align: Option<String>,
    pub v_align: Option<String>,
    pub x: Option<Emu>,
    pub y: Option<Emu>,
    pub wrap: Option<String>,
    pub dist_top: Option<Emu>,
    pub dist_bottom: Option<Emu>,
    pub dist_left: Option<Emu>,
    pub dist_right: Option<Emu>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub name: String,
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub width: Option<Emu>,
    pub height: Option<Emu>,
    pub alt: Option<String>,
    pub anchor: Option<Anchor>,
    /// Cropped fractions of the image, in OOXML hundred-thousandths: left, top, right, bottom.
    pub crop: [i32; 4],
    /// Inline effect extents in left, top, right, bottom order.
    pub effect_extent: Option<[Emu; 4]>,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum InlineObject {
    Field(Field),
    /// The beginning of a field whose cached result contains independently styled content.
    FieldStart {
        instr: String,
        dirty: bool,
        locked: bool,
    },
    /// The end of a field range, potentially in a later paragraph.
    FieldEnd,
    Orphan,
    Picture(Picture),
    Opaque(String),
    /// Opening or closing markup around an inline content control's visible runs.
    InlineControlBoundary(String),
    Hyperlink {
        target: String,
    },
    Bookmark {
        name: String,
        start: bool,
    },
    /// A note or comment reference, or the number marker within a note story.
    Note {
        kind: String,
        id: i32,
    },
    /// The separator line in a note separator story.
    NoteSeparator {
        continuation: bool,
    },
    /// An inline tab aligned to the page margins or paragraph indents (`w:ptab`).
    PositionalTab {
        alignment: String,
        relative_to: String,
        leader: Option<String>,
    },
    /// An Office Math equation. `latex` has no `$` delimiters. `display` is an `m:oMathPara`.
    Math {
        raw: String,
        latex: String,
        display: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub len: usize,
    pub props: CharProps,
}

#[derive(Debug, Clone)]
pub struct Paragraph {
    pub text: String,
    pub runs: Vec<Run>,
    pub objects: Vec<InlineObject>,
    pub props: ParaProps,
    pub retention: Retention,
    /// Original markup kept until an edit has consent to drop it.
    pub raw_xml: Option<String>,
    pub section: Option<SectionBreak>,
    pub locked: bool,
    pub links: Vec<String>,
    rev: u64,
}

/// Equality compares the content, not the internal change counter.
impl PartialEq for Paragraph {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
            && self.runs == other.runs
            && self.objects == other.objects
            && self.props == other.props
            && self.retention == other.retention
            && self.raw_xml == other.raw_xml
            && self.section == other.section
            && self.locked == other.locked
            && self.links == other.links
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionBreak {
    pub props: crate::section::SectionProps,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self::new()
    }
}

impl Paragraph {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            runs: Vec::new(),
            objects: Vec::new(),
            props: ParaProps::default(),
            retention: Retention::default(),
            raw_xml: None,
            section: None,
            locked: false,
            links: Vec::new(),
            rev: next_rev(),
        }
    }

    pub fn style(mut self, id: &str) -> Self {
        self.props.style = Some(id.to_string());
        self
    }

    pub fn text(mut self, text: &str) -> Self {
        self.push_text(text, &CharProps::default());
        self
    }

    pub fn merge_field(mut self, name: &str) -> Self {
        let instr = crate::fields::quote_name(name);
        let result = crate::fields::placeholder(name);
        self.push_object(
            InlineObject::Field(Field::new(&instr, &result)),
            &CharProps::default(),
        );
        self
    }

    /// A merge field whose cached result carries the given character properties.
    pub fn merge_field_styled(mut self, name: &str, props: &CharProps) -> Self {
        let instr = crate::fields::quote_name(name);
        let result = crate::fields::placeholder(name);
        self.push_object(InlineObject::Field(Field::new(&instr, &result)), props);
        self
    }

    pub fn field_instr(mut self, instr: &str) -> Self {
        self.push_object(
            InlineObject::Field(Field::new(instr, "")),
            &CharProps::default(),
        );
        self
    }

    pub fn align(mut self, align: &str) -> Self {
        self.props.align = Some(align.to_string());
        self
    }

    /// A paragraph that carries a full property set, used by the style writer.
    pub fn with_props(props: ParaProps) -> Self {
        let mut para = Self::new();
        para.props = props;
        para
    }

    pub fn indent<T: IntoTwips>(mut self, left: T) -> Self {
        self.props.indent_left = Some(left.twips());
        self
    }

    pub fn indent_right<T: IntoTwips>(mut self, right: T) -> Self {
        self.props.indent_right = Some(right.twips());
        self
    }

    pub fn indent_first<T: IntoTwips>(mut self, first: T) -> Self {
        self.props.indent_first = Some(first.twips());
        self
    }

    pub fn spacing_before<T: IntoTwips>(mut self, before: T) -> Self {
        self.props.spacing_before = Some(before.twips());
        self
    }

    pub fn spacing_after<T: IntoTwips>(mut self, after: T) -> Self {
        self.props.spacing_after = Some(after.twips());
        self
    }

    pub fn line_spacing<T: IntoTwips>(mut self, line: T, rule: &str) -> Self {
        self.props.line = Some(line.twips());
        self.props.line_rule = Some(rule.to_string());
        self
    }

    pub fn keep_next(mut self) -> Self {
        self.props.keep_next = true;
        self
    }

    pub fn keep_lines(mut self) -> Self {
        self.props.keep_lines = true;
        self
    }

    pub fn page_break_before(mut self) -> Self {
        self.props.page_break_before = true;
        self
    }

    pub fn page_break(mut self) -> Self {
        self.push_text(&PAGE_BREAK.to_string(), &CharProps::default());
        self
    }

    /// End the section at this paragraph. The next block starts on a new section.
    pub fn section_break(mut self, props: crate::section::SectionProps) -> Self {
        self.section = Some(SectionBreak { props });
        self
    }

    pub fn rtl(mut self) -> Self {
        self.props.bidi = true;
        self
    }

    pub fn shading(mut self, fill: &str) -> Self {
        self.props.shading = Some(fill.trim_start_matches('#').to_string());
        self
    }

    pub fn borders(mut self, borders: Borders) -> Self {
        self.props.borders = borders;
        self.props.borders_present = true;
        self
    }

    pub fn tab_stop(mut self, stop: TabStop) -> Self {
        self.props.tabs.push(stop);
        self
    }

    /// A list item. The writer generates the numbering definition.
    pub fn list(mut self, kind: ListKind, level: u32) -> Self {
        self.props.list = Some(ListSpec { kind, level });
        self
    }

    pub fn outline(mut self, level: i32) -> Self {
        self.props.outline = Some(level);
        self
    }

    pub fn bookmark(mut self, name: &str) -> Self {
        self.push_object(
            InlineObject::Bookmark {
                name: name.to_string(),
                start: true,
            },
            &CharProps::default(),
        );
        self
    }

    pub fn hyperlink(mut self, target: &str) -> Self {
        self.push_object(
            InlineObject::Hyperlink {
                target: target.to_string(),
            },
            &CharProps::default(),
        );
        self
    }

    pub fn picture(mut self, picture: Picture) -> Self {
        self.push_object(InlineObject::Picture(picture), &CharProps::default());
        self
    }

    pub fn run(mut self, run: RunBuilder) -> Self {
        if !run.text.is_empty() {
            self.push_text(&run.text, &run.props);
        }
        self
    }

    pub fn revision(&self) -> u64 {
        self.rev
    }

    pub fn run_builder(self, run: RunBuilder) -> ModelResult<Self> {
        let mut para = self;
        if let Some(err) = run.error {
            return Err(err);
        }
        if !run.text.is_empty() {
            para.push_text(&run.text, &run.props);
        }
        Ok(para)
    }

    pub fn push_text(&mut self, text: &str, props: &CharProps) {
        let clean: String = text.chars().filter(|c| *c != OBJ).collect();
        if clean.is_empty() {
            return;
        }
        // A run that ends with an object marker stays alone. If text joined it, the
        // writer could no longer tell the object position from the text.
        let after_object = self.text.ends_with(OBJ);
        if let Some(last) = self.runs.last_mut() {
            if !after_object && last.props == *props {
                last.len += clean.len();
                self.text.push_str(&clean);
                self.touch();
                return;
            }
        }
        self.runs.push(Run {
            len: clean.len(),
            props: props.clone(),
        });
        self.text.push_str(&clean);
        self.touch();
    }

    pub fn push_object(&mut self, object: InlineObject, props: &CharProps) {
        let marker = OBJ.to_string();
        self.runs.push(Run {
            len: marker.len(),
            props: props.clone(),
        });
        self.text.push_str(&marker);
        self.objects.push(object);
        self.touch();
    }

    pub fn insert_text(&mut self, offset: usize, text: &str, props: &CharProps) -> ModelResult<()> {
        self.check_boundary(offset)?;
        let clean: String = text.chars().filter(|c| *c != OBJ).collect();
        if clean.is_empty() {
            return Ok(());
        }
        self.text.insert_str(offset, &clean);
        self.retarget_runs(offset, clean.len() as isize, props, true);
        self.raw_xml = None;
        self.isolate_objects();
        self.touch();
        Ok(())
    }

    /// Delete `[start, end)` and the objects whose marker sits in that range.
    pub fn delete_range(&mut self, start: usize, end: usize) -> ModelResult<()> {
        self.check_boundary(start)?;
        self.check_boundary(end)?;
        if end > self.text.len() || start > end {
            return Err(ModelError::new(
                "model.bad_offset",
                "The delete range does not sit inside the paragraph.",
            ));
        }
        if start == end {
            return Ok(());
        }
        let old_len = self.text.len();
        let old_runs = self.runs.clone();
        self.objects = objects_outside(&self.text, &self.objects, start, end);
        let rest = self.text[end..].to_string();
        self.text.truncate(start);
        self.text.push_str(&rest);
        self.runs = join_runs(
            runs_covering(&old_runs, 0, start),
            runs_covering(&old_runs, end, old_len),
        );
        self.raw_xml = None;
        self.isolate_objects();
        self.touch();
        Ok(())
    }

    /// Split this paragraph at `offset`. The right-hand paragraph keeps the paragraph properties.
    pub fn split_off(&mut self, offset: usize) -> ModelResult<Paragraph> {
        self.check_boundary(offset)?;
        let old_len = self.text.len();
        let old_runs = self.runs.clone();
        let right_text = self.text[offset..].to_string();
        let keep = self.text[..offset].chars().filter(|c| *c == OBJ).count();
        let right_objects = self.objects.split_off(keep.min(self.objects.len()));
        self.text.truncate(offset);
        self.runs = runs_covering(&old_runs, 0, offset);
        let mut right = Paragraph::with_props(self.props.clone());
        right.text = right_text;
        right.objects = right_objects;
        right.runs = runs_covering(&old_runs, offset, old_len);
        right.links = self.links.clone();
        self.raw_xml = None;
        right.raw_xml = None;
        self.isolate_objects();
        right.isolate_objects();
        self.touch();
        right.touch();
        Ok(right)
    }

    /// Insert an inline object at a character boundary.
    pub fn insert_object(
        &mut self,
        offset: usize,
        object: InlineObject,
        props: &CharProps,
    ) -> ModelResult<()> {
        self.check_boundary(offset)?;
        let index = self.text[..offset].chars().filter(|c| *c == OBJ).count();
        let marker = OBJ.to_string();
        self.text.insert_str(offset, &marker);
        self.retarget_runs(offset, marker.len() as isize, props, false);
        if index > self.objects.len() {
            self.objects.push(object);
        } else {
            self.objects.insert(index, object);
        }
        self.raw_xml = None;
        self.isolate_objects();
        self.touch();
        Ok(())
    }

    /// Apply `change` to every run that covers `[start, end)`.
    pub fn update_range<F>(&mut self, start: usize, end: usize, mut change: F) -> ModelResult<()>
    where
        F: FnMut(&mut CharProps),
    {
        self.check_boundary(start)?;
        self.check_boundary(end)?;
        if start > end || end > self.text.len() {
            return Err(ModelError::new(
                "model.bad_offset",
                "The format range does not sit inside the paragraph.",
            ));
        }
        if start == end {
            return Ok(());
        }
        self.split_run_at(start);
        self.split_run_at(end);
        let mut cursor = 0usize;
        for run in &mut self.runs {
            let run_end = cursor + run.len;
            if run.len > 0 && cursor >= start && run_end <= end {
                change(&mut run.props);
            }
            cursor = run_end;
        }
        self.coalesce_runs();
        self.raw_xml = None;
        self.touch();
        Ok(())
    }

    /// The character properties a typed character inherits at `offset`.
    pub fn props_at(&self, offset: usize) -> CharProps {
        if !self.text.is_char_boundary(offset) {
            return CharProps::default();
        }
        let mut cursor = 0usize;
        let mut last = CharProps::default();
        for run in &self.runs {
            let end = cursor + run.len;
            if offset > cursor && offset <= end {
                return run.props.clone();
            }
            if run.len > 0 {
                last = run.props.clone();
            }
            cursor = end;
            if cursor >= offset {
                break;
            }
        }
        last
    }

    /// True when every run in `[start, end)` has `pred` true. An empty range is false.
    pub fn range_all<F>(&self, start: usize, end: usize, mut pred: F) -> bool
    where
        F: FnMut(&CharProps) -> bool,
    {
        if start >= end || !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
            return false;
        }
        let mut cursor = 0usize;
        let mut any = false;
        for run in &self.runs {
            let run_end = cursor + run.len;
            if run.len > 0 && run_end > start && cursor < end && !pred(&run.props) {
                return false;
            }
            if run.len > 0 && run_end > start && cursor < end {
                any = true;
            }
            cursor = run_end;
        }
        any
    }

    /// Append another paragraph's text, runs, and objects. Paragraph properties stay.
    pub fn append_paragraph(&mut self, other: &Paragraph) {
        if self.text.is_empty() {
            self.text.clone_from(&other.text);
            self.runs.clone_from(&other.runs);
            self.objects.clone_from(&other.objects);
        } else {
            self.text.push_str(&other.text);
            self.objects.extend(other.objects.iter().cloned());
            for run in &other.runs {
                self.push_merged(run.clone(), false);
            }
        }
        self.raw_xml = None;
        self.isolate_objects();
        self.touch();
    }

    /// The object at the marker that starts at `offset`, when one does.
    pub fn object_at(&self, offset: usize) -> Option<&InlineObject> {
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        if !self.text[offset..].starts_with(OBJ) {
            return None;
        }
        let index = self.text[..offset].chars().filter(|c| *c == OBJ).count();
        self.objects.get(index)
    }

    /// Keep every object marker in a run of its own, which is what the writer requires.
    fn isolate_objects(&mut self) {
        if !self.text.contains(OBJ) {
            return;
        }
        let old = std::mem::take(&mut self.runs);
        let mut rebuilt = Vec::new();
        let mut cursor = 0usize;
        for run in old {
            let end = (cursor + run.len).min(self.text.len());
            let Some(slice) = self.text.get(cursor..end) else {
                cursor = end;
                continue;
            };
            let mut local = 0usize;
            while local < slice.len() {
                if slice[local..].starts_with(OBJ) {
                    rebuilt.push(Run {
                        len: OBJ.len_utf8(),
                        props: run.props.clone(),
                    });
                    local += OBJ.len_utf8();
                    continue;
                }
                let next = slice[local..]
                    .find(OBJ)
                    .map_or(slice.len(), |at| local + at);
                rebuilt.push(Run {
                    len: next - local,
                    props: run.props.clone(),
                });
                local = next;
            }
            cursor = end;
        }
        let covered: usize = rebuilt.iter().map(|run| run.len).sum();
        if covered < self.text.len() {
            rebuilt.push(Run {
                len: self.text.len() - covered,
                props: CharProps::default(),
            });
        }
        self.runs = rebuilt;
    }

    fn check_boundary(&self, offset: usize) -> ModelResult<()> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return Err(ModelError::new(
                "model.bad_offset",
                format!(
                    "The offset {offset} is not a character boundary. Choose a boundary in the paragraph."
                ),
            ));
        }
        Ok(())
    }

    fn split_run_at(&mut self, offset: usize) {
        let mut cursor = 0usize;
        let mut insert_at = None;
        for (index, run) in self.runs.iter().enumerate() {
            let end = cursor + run.len;
            if offset > cursor && offset < end {
                insert_at = Some((index, offset - cursor, end - offset, run.props.clone()));
                break;
            }
            cursor = end;
        }
        if let Some((index, before, after, props)) = insert_at {
            if let Some(run) = self.runs.get_mut(index) {
                run.len = before;
            }
            self.runs.insert(index + 1, Run { len: after, props });
        }
    }

    fn coalesce_runs(&mut self) {
        let old = std::mem::take(&mut self.runs);
        let mut cursor = 0usize;
        for run in old {
            let end = cursor + run.len;
            let object = self
                .text
                .get(cursor..end)
                .is_some_and(|slice| slice == "\u{FFFC}");
            let covered: usize = self.runs.iter().map(|run| run.len).sum();
            let previous_object = self.runs.last().is_some_and(|last| {
                let start = covered.saturating_sub(last.len);
                self.text
                    .get(start..covered)
                    .is_some_and(|slice| slice == "\u{FFFC}")
            });
            self.push_merged(run, !object && !previous_object);
            cursor = end;
        }
    }

    fn retarget_runs(&mut self, offset: usize, delta: isize, props: &CharProps, coalesce: bool) {
        if delta <= 0 {
            return;
        }
        let add = delta as usize;
        let old = std::mem::take(&mut self.runs);
        let mut cursor = 0usize;
        let mut placed = false;
        for run in old {
            let end = cursor + run.len;
            if !placed && offset <= end {
                let before = offset.saturating_sub(cursor);
                let after = end.saturating_sub(offset);
                if before > 0 {
                    self.push_merged(
                        Run {
                            len: before,
                            props: run.props.clone(),
                        },
                        true,
                    );
                }
                self.push_merged(
                    Run {
                        len: add,
                        props: props.clone(),
                    },
                    coalesce,
                );
                if after > 0 {
                    self.push_merged(
                        Run {
                            len: after,
                            props: run.props,
                        },
                        coalesce,
                    );
                }
                placed = true;
            } else {
                self.push_merged(run, true);
            }
            cursor = end;
        }
        if !placed {
            self.push_merged(
                Run {
                    len: add,
                    props: props.clone(),
                },
                coalesce,
            );
        }
    }

    fn push_merged(&mut self, run: Run, coalesce: bool) {
        if run.len == 0 {
            return;
        }
        if coalesce {
            if let Some(last) = self.runs.last_mut() {
                if last.props == run.props {
                    last.len += run.len;
                    return;
                }
            }
        }
        self.runs.push(run);
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        let mut objects = self.objects.iter();
        for ch in self.text.chars() {
            if ch == OBJ {
                match objects.next() {
                    Some(InlineObject::Field(field)) => out.push_str(&field.result),
                    Some(InlineObject::Hyperlink { .. }) => {}
                    Some(InlineObject::PositionalTab { .. }) => out.push('\t'),
                    Some(InlineObject::Math { latex, .. }) => out.push_str(latex),
                    _ => {}
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// Replace the object at `index` with text, and resize the run that held it.
    ///
    /// The writer slices the text of each run by the run length. A field result can be
    /// longer or shorter than the object character, so the run length must change with it.
    pub fn replace_object(&mut self, index: usize, text: &str) -> bool {
        if index >= self.objects.len() {
            return false;
        }
        // The byte offset of the k-th object character.
        let mut offset = None;
        let mut seen = 0usize;
        for (byte, ch) in self.text.char_indices() {
            if ch == OBJ {
                if seen == index {
                    offset = Some(byte);
                    break;
                }
                seen += 1;
            }
        }
        let Some(offset) = offset else { return false };

        // The run that covers that byte, found before the text changes.
        let mut cursor = 0usize;
        let mut run_at = None;
        for (position, run) in self.runs.iter().enumerate() {
            let end = cursor + run.len;
            if offset >= cursor && offset < end {
                run_at = Some(position);
                break;
            }
            cursor = end;
        }

        self.objects.remove(index);
        // `text` holds no object character, so the object run becomes one plain run.
        self.text
            .replace_range(offset..offset + OBJ.len_utf8(), text);
        if let Some(position) = run_at {
            if let Some(run) = self.runs.get_mut(position) {
                run.len = text.len();
            }
        }
        self.touch();
        true
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn pictures(&self) -> impl Iterator<Item = &Picture> {
        self.objects.iter().filter_map(|object| match object {
            InlineObject::Picture(picture) => Some(picture),
            _ => None,
        })
    }

    fn touch(&mut self) {
        self.rev = next_rev();
    }
}

#[derive(Debug, Clone)]
pub struct RunBuilder {
    pub text: String,
    pub props: CharProps,
    pub error: Option<ModelError>,
}

impl RunBuilder {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            props: CharProps::default(),
            error: None,
        }
    }

    pub fn text(mut self, text: &str) -> Self {
        self.text.push_str(text);
        self
    }

    pub fn bold(mut self) -> Self {
        self.props.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.props.italic = true;
        self
    }

    pub fn underline(mut self) -> Self {
        self.props.underline = true;
        self
    }

    pub fn strike(mut self) -> Self {
        self.props.strike = true;
        self
    }

    pub fn caps(mut self) -> Self {
        self.props.caps = true;
        self
    }

    pub fn small_caps(mut self) -> Self {
        self.props.small_caps = true;
        self
    }

    pub fn superscript(mut self) -> Self {
        self.props.vert_align = Some("superscript".into());
        self
    }

    pub fn subscript(mut self) -> Self {
        self.props.vert_align = Some("subscript".into());
        self
    }

    pub fn size(mut self, pt: edgedoc_unit::Pt) -> Self {
        if let Err(err) = edgedoc_limit::Budget::default().font_size(pt.0) {
            self.error = Some(ModelError::new(err.code, err.message));
        } else {
            self.props.size_pt = Some(pt.0);
        }
        self
    }

    pub fn font(mut self, name: &str) -> Self {
        self.props.font = Some(name.to_string());
        self
    }

    pub fn color(mut self, hex: &str) -> Self {
        self.props.color = Some(hex.trim_start_matches('#').to_string());
        self
    }

    pub fn style(mut self, id: &str) -> Self {
        self.props.style = Some(id.to_string());
        self
    }

    pub fn highlight(mut self, name: &str) -> Self {
        self.props.highlight = Some(name.to_string());
        self
    }

    pub fn shading(mut self, fill: &str) -> Self {
        self.props.shading = Some(fill.trim_start_matches('#').to_string());
        self
    }

    pub fn font_ea(mut self, name: &str) -> Self {
        self.props.font_ea = Some(name.to_string());
        self
    }

    pub fn vanish(mut self) -> Self {
        self.props.vanish = true;
        self
    }

    pub fn rtl(mut self) -> Self {
        self.props.rtl = true;
        self
    }

    pub fn lang(mut self, lang: &str) -> Self {
        self.props.lang = Some(lang.to_string());
        self
    }

    pub fn spacing(mut self, twentieths: i32) -> Self {
        self.props.spacing = Some(twentieths);
        self
    }

    /// Squeeze the text into a width, as `w:fitText` does.
    pub fn fit_text(mut self, width: edgedoc_unit::Twips) -> Self {
        self.props.fit_text = Some(width);
        self
    }
}

impl Default for RunBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Picture {
    pub fn new(name: &str, bytes: Vec<u8>, content_type: &str) -> Self {
        Self {
            name: name.to_string(),
            bytes,
            content_type: content_type.to_string(),
            width: None,
            height: None,
            alt: None,
            anchor: None,
            crop: [0; 4],
            effect_extent: None,
        }
    }

    pub fn png(bytes: Vec<u8>) -> Self {
        Self::new("image.png", bytes, "image/png")
    }

    pub fn jpeg(bytes: Vec<u8>) -> Self {
        Self::new("image.jpeg", bytes, "image/jpeg")
    }

    pub fn size(mut self, width: Emu, height: Emu) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    pub fn alt(mut self, text: &str) -> Self {
        self.alt = Some(text.to_string());
        self
    }

    pub fn floating(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }
}

impl Anchor {
    pub fn inline_text_distance() -> Self {
        Self {
            wrap: Some("square".into()),
            ..Self::default()
        }
    }

    pub fn behind_text(mut self) -> Self {
        self.behind_text = true;
        self
    }

    pub fn wrap(mut self, wrap: &str) -> Self {
        self.wrap = Some(wrap.to_string());
        self
    }

    pub fn align(mut self, horizontal: &str, vertical: &str) -> Self {
        self.h_align = Some(horizontal.to_string());
        self.v_align = Some(vertical.to_string());
        self
    }

    pub fn offset(mut self, x: Emu, y: Emu) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }

    pub fn distance<T: IntoTwips>(mut self, all: T) -> Self {
        let tw = all.twips();
        let emu = Emu(i64::from(tw.0) * 635);
        self.dist_top = Some(emu);
        self.dist_bottom = Some(emu);
        self.dist_left = Some(emu);
        self.dist_right = Some(emu);
        self
    }
}

fn objects_outside(
    text: &str,
    objects: &[InlineObject],
    start: usize,
    end: usize,
) -> Vec<InlineObject> {
    let mut kept = Vec::new();
    let mut seen = 0usize;
    for (byte, ch) in text.char_indices() {
        if ch != OBJ {
            continue;
        }
        if (byte < start || byte >= end) && seen < objects.len() {
            kept.push(objects[seen].clone());
        }
        seen += 1;
    }
    kept
}

fn runs_covering(runs: &[Run], start: usize, end: usize) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut cursor = 0usize;
    for run in runs {
        let run_end = cursor + run.len;
        let from = start.max(cursor);
        let to = end.min(run_end);
        if to > from {
            let piece = Run {
                len: to - from,
                props: run.props.clone(),
            };
            out.push(piece);
        }
        cursor = run_end;
        if cursor >= end {
            break;
        }
    }
    out
}

fn join_runs(mut left: Vec<Run>, right: Vec<Run>) -> Vec<Run> {
    for run in right {
        if let Some(last) = left.last_mut() {
            if last.props == run.props {
                last.len += run.len;
                continue;
            }
        }
        if run.len > 0 {
            left.push(run);
        }
    }
    left
}

/// A run whose text is a tab character.
pub const TAB: &str = "\t";

/// A run whose text is a line break.
pub const LINE_BREAK: &str = "\n";
