use edgedoc_unit::{IntoTwips, Mm, Twips};

use crate::table::Borders;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeaderRefs {
    pub default: Option<u32>,
    pub first: Option<u32>,
    pub even: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    Header,
    Footer,
    Footnote,
    Endnote,
    Comment,
    Other,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineNumbering {
    pub count_by: u32,
    pub start: u32,
    pub restart: Option<String>,
    pub distance: Option<Twips>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocGrid {
    pub grid_type: Option<String>,
    pub line_pitch: Option<i32>,
    pub char_space: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FootnotePr {
    pub num_fmt: Option<String>,
    pub num_start: Option<u32>,
    pub num_restart: Option<String>,
    pub pos: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectionProps {
    pub page_w: Twips,
    pub page_h: Twips,
    pub margin_top: Twips,
    pub margin_bottom: Twips,
    pub margin_left: Twips,
    pub margin_right: Twips,
    pub header_dist: Twips,
    pub footer_dist: Twips,
    pub gutter: Twips,
    pub landscape: bool,
    pub columns: u32,
    pub column_space: Option<Twips>,
    pub separate_columns: bool,
    pub title_page: bool,
    pub bidi: bool,
    pub rtl_gutter: bool,
    pub text_direction: Option<String>,
    /// `w:vAlign`: where the text sits between the top and bottom margins.
    pub v_align: Option<String>,
    pub line_numbering: Option<LineNumbering>,
    pub page_borders: Borders,
    pub doc_grid: Option<DocGrid>,
    pub footnotes: FootnotePr,
    pub endnotes: FootnotePr,
    pub headers: HeaderRefs,
    pub footers: HeaderRefs,
    pub page_start: Option<u32>,
    pub page_fmt: Option<String>,
    pub start: SectionStart,
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionStart {
    NextPage,
    Continuous,
    OddPage,
    EvenPage,
}

impl Default for SectionProps {
    fn default() -> Self {
        let (w, h) = edgedoc_unit::a4();
        let margin = Mm(25.4).twips();
        Self {
            page_w: w,
            page_h: h,
            margin_top: margin,
            margin_bottom: margin,
            margin_left: margin,
            margin_right: margin,
            header_dist: Twips(708),
            footer_dist: Twips(708),
            gutter: Twips(0),
            landscape: false,
            columns: 1,
            column_space: None,
            separate_columns: false,
            title_page: false,
            bidi: false,
            rtl_gutter: false,
            text_direction: None,
            v_align: None,
            line_numbering: None,
            page_borders: Borders::default(),
            doc_grid: None,
            footnotes: FootnotePr::default(),
            endnotes: FootnotePr::default(),
            headers: HeaderRefs::default(),
            footers: HeaderRefs::default(),
            page_start: None,
            page_fmt: None,
            start: SectionStart::NextPage,
            extra: Vec::new(),
        }
    }
}

impl SectionProps {
    pub fn margins<T: IntoTwips>(mut self, all: T) -> Self {
        let tw = all.twips();
        self.margin_top = tw;
        self.margin_bottom = tw;
        self.margin_left = tw;
        self.margin_right = tw;
        self
    }

    pub fn header<T: IntoTwips>(mut self, distance: T) -> Self {
        self.header_dist = distance.twips();
        self
    }

    pub fn footer<T: IntoTwips>(mut self, distance: T) -> Self {
        self.footer_dist = distance.twips();
        self
    }

    pub fn gutter<T: IntoTwips>(mut self, size: T) -> Self {
        self.gutter = size.twips();
        self
    }

    pub fn columns(mut self, count: u32) -> Self {
        self.columns = count.max(1);
        self
    }

    pub fn title_page(mut self) -> Self {
        self.title_page = true;
        self
    }

    pub fn landscape(mut self) -> Self {
        self.landscape = true;
        let (w, h) = (self.page_w, self.page_h);
        self.page_w = h;
        self.page_h = w;
        self
    }

    /// Mirror margins, gutter on the inside edge, and right-to-left text.
    pub fn rtl(mut self) -> Self {
        self.bidi = true;
        self.rtl_gutter = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub kind: PartKind,
    pub blocks: Vec<std::sync::Arc<crate::table::Block>>,
    /// Note and comment identity: `w:id`, author, date, and the mark text.
    pub meta: PartMeta,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PartMeta {
    pub id: Option<i32>,
    pub author: Option<String>,
    pub date: Option<String>,
    pub initials: Option<String>,
    /// The type attribute of a note: `normal`, `separator`, or `continuationSeparator`.
    pub note_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryRef {
    Body,
    Part(u32),
}
