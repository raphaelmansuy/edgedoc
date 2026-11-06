use edgedoc_unit::Twips;

use crate::table::{Borders, LineBorder};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharProps {
    pub bold: bool,
    pub italic: bool,
    /// WordprocessingML on/off properties explicitly disabled at this level.
    /// An absent property inherits; an explicit off overrides an inherited value.
    pub explicit_off: Vec<String>,
    pub underline: bool,
    pub strike: bool,
    pub double_strike: bool,
    pub size_pt: Option<f32>,
    pub font: Option<String>,
    pub color: Option<String>,
    pub highlight: Option<String>,
    pub rtl: bool,
    pub complex_script: bool,
    pub vanish: bool,
    pub web_hidden: bool,
    pub style: Option<String>,
    pub vert_align: Option<String>,
    pub lang: Option<String>,
    pub lang_bidi: Option<String>,
    pub lang_ea: Option<String>,
    pub font_hansi: Option<String>,
    pub font_cs: Option<String>,
    pub font_ea: Option<String>,
    pub shading: Option<String>,
    pub underline_val: Option<String>,
    pub underline_color: Option<String>,
    pub size_cs: Option<f32>,
    pub link: Option<String>,
    pub caps: bool,
    pub small_caps: bool,
    pub emboss: bool,
    pub imprint: bool,
    pub outline: bool,
    pub shadow: bool,
    pub no_proof: bool,
    pub snap_to_grid: Option<bool>,
    pub bold_cs: Option<bool>,
    pub italic_cs: Option<bool>,
    /// Letter spacing in twentieths of a point, as `w:spacing/@w:val` stores it.
    pub spacing: Option<i32>,
    /// Kerning threshold in half-points, as `w:kern/@w:val` stores it.
    pub kern: Option<u32>,
    /// Vertical offset in half-points, as `w:position/@w:val` stores it.
    pub position: Option<i32>,
    /// Character width in percent, as `w:w/@w:val` stores it.
    pub scale: Option<u32>,
    pub border: Option<LineBorder>,
    /// `ins` or `del` when the run sits inside a tracked change.
    pub revision: Option<String>,
    /// `w:eastAsianLayout`: two lines in one, or horizontal text in a vertical run.
    pub east_asian_layout: Option<EastAsianLayout>,
    /// `w:fitText/@w:val`: the width in twips that the text is squeezed into.
    pub fit_text: Option<Twips>,
    /// Run property elements that this model does not name. They are kept in order.
    pub extra: Vec<String>,
}

impl CharProps {
    pub fn is_empty(&self) -> bool {
        *self == CharProps::default()
    }
}

/// `w:eastAsianLayout`. Each field maps to one attribute of the element.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EastAsianLayout {
    pub id: Option<u32>,
    /// `w:combine`: two lines in the space of one.
    pub combine: Option<bool>,
    /// `w:combineBrackets`: which brackets the combined text takes.
    pub combine_brackets: Option<String>,
    /// `w:vert`: rotate the text in a vertical line.
    pub vertical: Option<bool>,
    /// `w:vertCompress`: squeeze rotated text into one line.
    pub vertical_compress: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabStop {
    pub val: String,
    pub pos: Twips,
    pub leader: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrameProps {
    pub drop_cap: Option<String>,
    pub lines: Option<u32>,
    pub wrap: Option<String>,
    pub x: Option<Twips>,
    pub y: Option<Twips>,
    pub h_anchor: Option<String>,
    pub v_anchor: Option<String>,
    pub x_align: Option<String>,
    pub y_align: Option<String>,
    pub h_space: Option<Twips>,
    pub v_space: Option<Twips>,
    pub width: Option<Twips>,
    pub height: Option<Twips>,
    /// Frame attributes that this model does not name.
    pub extra: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParaProps {
    pub style: Option<String>,
    pub align: Option<String>,
    pub indent_left: Option<Twips>,
    pub indent_right: Option<Twips>,
    pub indent_first: Option<Twips>,
    /// Character indents in OOXML hundredths of a character.
    pub indent_left_chars: Option<i32>,
    pub indent_right_chars: Option<i32>,
    pub indent_first_chars: Option<i32>,
    pub spacing_before: Option<Twips>,
    pub spacing_after: Option<Twips>,
    pub spacing_before_auto: Option<bool>,
    pub spacing_after_auto: Option<bool>,
    pub line: Option<Twips>,
    pub line_rule: Option<String>,
    pub num_id: Option<u32>,
    pub ilvl: Option<u32>,
    pub list: Option<ListSpec>,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub contextual_spacing: bool,
    pub suppress_line_numbers: bool,
    /// On/off elements explicitly present in the source, including values set to off.
    /// Their names let a paragraph override a value inherited from a style.
    pub explicit_on_off: Vec<String>,
    pub outline: Option<i32>,
    pub bidi: bool,
    /// East Asian punctuation rules; absent inherits the style setting.
    pub kinsoku: Option<bool>,
    /// Whether Latin words stay together; absent inherits the style setting.
    pub word_wrap: Option<bool>,
    pub shading: Option<String>,
    pub borders: Borders,
    /// Whether an explicit paragraph-border set was supplied, including an empty set.
    pub borders_present: bool,
    pub tabs: Vec<TabStop>,
    pub frame: Option<FrameProps>,
    pub mark: CharProps,
    /// Paragraph property elements that this model does not name. They are kept in order.
    pub extra: Vec<String>,
}

/// A list style that the writer builds in the numbering part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Decimal,
    LowerRoman,
    UpperRoman,
    LowerLetter,
    UpperLetter,
}

impl ListKind {
    pub fn num_fmt(self) -> &'static str {
        match self {
            ListKind::Bullet => "bullet",
            ListKind::Decimal => "decimal",
            ListKind::LowerRoman => "lowerRoman",
            ListKind::UpperRoman => "upperRoman",
            ListKind::LowerLetter => "lowerLetter",
            ListKind::UpperLetter => "upperLetter",
        }
    }

    pub fn level_text(self, level: u32) -> String {
        match self {
            ListKind::Bullet => "\u{2022}".into(),
            _ => format!("%{}.", level + 1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListSpec {
    pub kind: ListKind,
    pub level: u32,
}

impl Default for ParaProps {
    fn default() -> Self {
        Self {
            style: None,
            align: None,
            indent_left: None,
            indent_right: None,
            indent_first: None,
            indent_left_chars: None,
            indent_right_chars: None,
            indent_first_chars: None,
            spacing_before: None,
            spacing_after: None,
            spacing_before_auto: None,
            spacing_after_auto: None,
            line: None,
            line_rule: None,
            num_id: None,
            ilvl: None,
            list: None,
            keep_next: false,
            keep_lines: false,
            page_break_before: false,
            widow_control: true,
            contextual_spacing: false,
            suppress_line_numbers: false,
            explicit_on_off: Vec::new(),
            outline: None,
            bidi: false,
            kinsoku: None,
            word_wrap: None,
            shading: None,
            borders: Borders::default(),
            borders_present: false,
            tabs: Vec::new(),
            frame: None,
            mark: CharProps::default(),
            extra: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Retention {
    pub revisions: bool,
    pub comments: bool,
    pub equations: bool,
}

impl Retention {
    pub fn any(&self) -> bool {
        self.revisions || self.comments || self.equations
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropKind {
    Revisions,
    Comments,
    Equations,
    Objects,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditReport {
    pub dropped: Vec<DropKind>,
    pub orphans_removed: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditOptions {
    pub allow_revisions: bool,
    pub allow_comments: bool,
    pub allow_equations: bool,
    pub allow_objects: bool,
}

impl EditOptions {
    pub fn allow(mut self, kind: DropKind) -> Self {
        match kind {
            DropKind::Revisions => self.allow_revisions = true,
            DropKind::Comments => self.allow_comments = true,
            DropKind::Equations => self.allow_equations = true,
            DropKind::Objects => self.allow_objects = true,
        }
        self
    }

    pub fn permits(&self, kind: DropKind) -> bool {
        match kind {
            DropKind::Revisions => self.allow_revisions,
            DropKind::Comments => self.allow_comments,
            DropKind::Equations => self.allow_equations,
            DropKind::Objects => self.allow_objects,
        }
    }
}
