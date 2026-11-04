use std::sync::Arc;

use edgedoc_unit::{IntoTwips, Twips};

use crate::para::Paragraph;

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub blocks: Vec<Arc<Block>>,
    pub span: usize,
    pub width: Option<Twips>,
    pub v_merge: Option<String>,
    pub h_merge: Option<String>,
    pub v_align: Option<String>,
    pub text_direction: Option<String>,
    pub shading: Option<String>,
    pub margins: CellMargins,
    pub borders: Borders,
    pub fit_text: bool,
    pub no_wrap: bool,
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub header: bool,
    pub height: Option<Twips>,
    pub height_rule: Option<String>,
    pub cant_split: bool,
    pub hidden: bool,
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineBorder {
    pub val: String,
    pub size: Option<i32>,
    pub color: Option<String>,
    pub space: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Borders {
    pub top: Option<LineBorder>,
    pub left: Option<LineBorder>,
    pub bottom: Option<LineBorder>,
    pub right: Option<LineBorder>,
    pub inside_h: Option<LineBorder>,
    pub inside_v: Option<LineBorder>,
    pub start: Option<LineBorder>,
    pub end: Option<LineBorder>,
    pub between: Option<LineBorder>,
    pub bar: Option<LineBorder>,
    pub tl2_br: Option<LineBorder>,
    pub tr2_bl: Option<LineBorder>,
}

impl Borders {
    pub fn is_empty(&self) -> bool {
        self.top.is_none()
            && self.left.is_none()
            && self.bottom.is_none()
            && self.right.is_none()
            && self.inside_h.is_none()
            && self.inside_v.is_none()
            && self.start.is_none()
            && self.end.is_none()
            && self.between.is_none()
            && self.bar.is_none()
            && self.tl2_br.is_none()
            && self.tr2_bl.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CellMargins {
    pub top: Option<Twips>,
    pub left: Option<Twips>,
    pub bottom: Option<Twips>,
    pub right: Option<Twips>,
}

/// `w:tblpPr`: a table that floats beside the text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TablePosition {
    pub left_from_text: Option<i32>,
    pub right_from_text: Option<i32>,
    pub top_from_text: Option<i32>,
    pub bottom_from_text: Option<i32>,
    pub left_from: Option<String>,
    pub top_from: Option<String>,
    pub h_anchor: Option<String>,
    pub v_anchor: Option<String>,
    pub tblp_x: Option<i32>,
    pub tblp_y: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub rows: Vec<Row>,
    pub widths: Vec<Twips>,
    /// `w:tblW`: the value and the unit, as `w:w` and `w:type` store them.
    pub width_kind: Option<String>,
    pub style: Option<String>,
    pub align: Option<String>,
    pub indent: Option<Twips>,
    pub layout: Option<String>,
    pub cell_space: Option<Twips>,
    pub bidi_visual: bool,
    pub overlap: Option<String>,
    pub position: Option<TablePosition>,
    pub borders: Borders,
    pub cell_margins: CellMargins,
    pub look: Option<String>,
    /// `w:tblPrEx` raw markup, and other table properties that the model does not name.
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    Opaque(String),
    Alternate {
        requires: String,
        choice: Vec<Arc<Block>>,
        fallback_xml: String,
    },
    Control(ContentControl),
}

/// One item of a drop-down list or a combo box.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContentItem {
    pub display: String,
    pub value: String,
}

impl ContentItem {
    pub fn new(display: &str, value: &str) -> Self {
        Self {
            display: display.to_string(),
            value: value.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContentControl {
    pub title: String,
    pub tag: String,
    pub lock_content: bool,
    pub kind: String,
    pub placeholder: String,
    pub binding_xml: Option<String>,
    pub blocks: Vec<Arc<Block>>,
    /// List items, date format, or checked state, per the control kind.
    pub items: Vec<ContentItem>,
    pub date_format: Option<String>,
    pub checked: Option<bool>,
}

impl Table {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            widths: Vec::new(),
            width_kind: None,
            style: None,
            align: None,
            indent: None,
            layout: None,
            cell_space: None,
            bidi_visual: false,
            overlap: None,
            position: None,
            borders: Borders::default(),
            cell_margins: CellMargins::default(),
            look: None,
            extra: Vec::new(),
        }
    }

    pub fn widths<T: IntoTwips + Copy>(mut self, widths: impl IntoIterator<Item = T>) -> Self {
        self.widths = widths.into_iter().map(|w| w.twips()).collect();
        self
    }

    pub fn header<'a>(mut self, cells: impl IntoIterator<Item = &'a str>) -> Self {
        self.rows.push(row_from(cells, true));
        self
    }

    pub fn row<'a>(mut self, cells: impl IntoIterator<Item = &'a str>) -> Self {
        self.rows.push(row_from(cells, false));
        self
    }

    /// A table style id, such as `TableGrid`.
    pub fn style(mut self, id: &str) -> Self {
        self.style = Some(id.to_string());
        self
    }

    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }

    pub fn cell_margins(mut self, margins: CellMargins) -> Self {
        self.cell_margins = margins;
        self
    }

    pub fn bidi(mut self) -> Self {
        self.bidi_visual = true;
        self
    }

    pub fn align(mut self, align: &str) -> Self {
        self.align = Some(align.to_string());
        self
    }

    pub fn indent<T: IntoTwips>(mut self, size: T) -> Self {
        self.indent = Some(size.twips());
        self
    }

    /// A table that wraps text around its frame.
    pub fn floating(mut self, position: TablePosition) -> Self {
        self.position = Some(position);
        self
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Row {
    pub fn cells(cells: impl IntoIterator<Item = Cell>) -> Self {
        Self {
            cells: cells.into_iter().collect(),
            header: false,
            height: None,
            height_rule: None,
            cant_split: false,
            hidden: false,
            extra: Vec::new(),
        }
    }

    pub fn height<T: IntoTwips>(mut self, height: T, rule: &str) -> Self {
        self.height = Some(height.twips());
        self.height_rule = Some(rule.to_string());
        self
    }

    pub fn header(mut self) -> Self {
        self.header = true;
        self
    }

    pub fn cant_split(mut self) -> Self {
        self.cant_split = true;
        self
    }
}

impl Cell {
    pub fn paragraph(text: &str) -> Self {
        Self {
            blocks: vec![Arc::new(Block::Paragraph(Paragraph::new().text(text)))],
            span: 1,
            width: None,
            v_merge: None,
            h_merge: None,
            v_align: None,
            text_direction: None,
            shading: None,
            margins: CellMargins::default(),
            borders: Borders::default(),
            fit_text: false,
            no_wrap: false,
            extra: Vec::new(),
        }
    }

    /// A cell whose blocks replace the starting paragraph, not follow it.
    pub fn blocks(mut self, blocks: impl IntoIterator<Item = Arc<Block>>) -> Self {
        let blocks: Vec<Arc<Block>> = blocks.into_iter().collect();
        if !blocks.is_empty() {
            self.blocks = blocks;
        }
        self
    }

    pub fn span(mut self, span: usize) -> Self {
        self.span = span.max(1);
        self
    }

    pub fn width<T: IntoTwips>(mut self, width: T) -> Self {
        self.width = Some(width.twips());
        self
    }

    pub fn shading(mut self, fill: &str) -> Self {
        self.shading = Some(fill.trim_start_matches('#').to_string());
        self
    }

    pub fn valign(mut self, align: &str) -> Self {
        self.v_align = Some(align.to_string());
        self
    }

    pub fn borders(mut self, borders: Borders) -> Self {
        self.borders = borders;
        self
    }
}

fn row_from<'a>(cells: impl IntoIterator<Item = &'a str>, header: bool) -> Row {
    Row {
        header,
        height: None,
        height_rule: None,
        cant_split: false,
        hidden: false,
        extra: Vec::new(),
        cells: cells.into_iter().map(Cell::paragraph).collect(),
    }
}

impl ContentControl {
    pub fn text(title: &str, value: &str) -> Self {
        Self::new(title, "text", value)
    }

    pub fn date(title: &str, value: &str, format: &str) -> Self {
        let mut control = Self::new(title, "date", value);
        control.date_format = Some(format.to_string());
        control
    }

    pub fn drop_down<'a>(
        title: &str,
        value: &str,
        items: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut control = Self::new(title, "dropDownList", value);
        control.items = items
            .into_iter()
            .map(|item| ContentItem::new(item, item))
            .collect();
        control
    }

    pub fn checked(title: &str, value: bool) -> Self {
        let mut control = Self::new(title, "checkbox", "");
        control.checked = Some(value);
        control
    }

    fn new(title: &str, kind: &str, value: &str) -> Self {
        Self {
            title: title.to_string(),
            tag: title.to_string(),
            lock_content: false,
            kind: kind.to_string(),
            placeholder: String::new(),
            binding_xml: None,
            blocks: vec![Arc::new(Block::Paragraph(Paragraph::new().text(value)))],
            items: Vec::new(),
            date_format: None,
            checked: None,
        }
    }

    pub fn items<'a>(mut self, items: impl IntoIterator<Item = &'a str>) -> Self {
        self.items = items
            .into_iter()
            .map(|item| ContentItem::new(item, item))
            .collect();
        self
    }
}

pub fn clamp_span(span: usize, max_cols: usize) -> usize {
    span.clamp(1, max_cols)
}
