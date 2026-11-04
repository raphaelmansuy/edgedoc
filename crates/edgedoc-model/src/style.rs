use edgedoc_unit::Pt;

use crate::props::{CharProps, ParaProps};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Paragraph,
    Character,
    Table,
    Numbering,
}

impl StyleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            StyleKind::Paragraph => "paragraph",
            StyleKind::Character => "character",
            StyleKind::Table => "table",
            StyleKind::Numbering => "numbering",
        }
    }

    pub fn parse(kind: &str) -> Self {
        match kind {
            "character" => StyleKind::Character,
            "table" => StyleKind::Table,
            "numbering" => StyleKind::Numbering,
            _ => StyleKind::Paragraph,
        }
    }
}

/// One `w:style` entry.
///
/// `raw_xml` holds the original element. The writer emits it when the style has no
/// modeled change, so table styles and vendor styles survive a save.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub id: String,
    pub name: String,
    pub kind: StyleKind,
    pub based_on: Option<String>,
    pub next: Option<String>,
    pub link: Option<String>,
    pub q_format: bool,
    pub semi_hidden: bool,
    pub priority: Option<i32>,
    pub char_props: CharProps,
    pub para_props: ParaProps,
    pub raw_xml: Option<String>,
}

impl Style {
    pub fn paragraph(id: &str) -> Self {
        Self::new(id, StyleKind::Paragraph)
    }

    pub fn character(id: &str) -> Self {
        Self::new(id, StyleKind::Character)
    }

    pub fn table(id: &str) -> Self {
        Self::new(id, StyleKind::Table)
    }

    fn new(id: &str, kind: StyleKind) -> Self {
        let para_props = ParaProps {
            outline: heading_level(id),
            ..ParaProps::default()
        };
        Self {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            based_on: None,
            next: None,
            link: None,
            q_format: true,
            semi_hidden: false,
            priority: None,
            char_props: CharProps::default(),
            para_props,
            raw_xml: None,
        }
    }

    pub fn size(mut self, pt: Pt) -> Self {
        self.char_props.size_pt = Some(pt.0);
        self
    }

    pub fn bold(mut self) -> Self {
        self.char_props.bold = true;
        self
    }

    pub fn italic(mut self) -> Self {
        self.char_props.italic = true;
        self
    }

    pub fn font(mut self, name: &str) -> Self {
        self.char_props.font = Some(name.to_string());
        self
    }

    pub fn color(mut self, hex: &str) -> Self {
        self.char_props.color = Some(hex.trim_start_matches('#').to_string());
        self
    }

    pub fn align(mut self, align: &str) -> Self {
        self.para_props.align = Some(align.to_string());
        self
    }

    pub fn named(mut self, name: &str) -> Self {
        self.name = name.to_string();
        if self.para_props.outline.is_none() {
            self.para_props.outline = heading_level(name);
        }
        self
    }

    pub fn based_on(mut self, id: &str) -> Self {
        self.based_on = Some(id.to_string());
        self
    }

    pub fn next(mut self, id: &str) -> Self {
        self.next = Some(id.to_string());
        self
    }

    pub fn chars(mut self, props: CharProps) -> Self {
        self.char_props = props;
        self
    }

    pub fn para(mut self, props: ParaProps) -> Self {
        self.para_props = props;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StyleSheet {
    pub styles: Vec<Style>,
    /// Raw `w:style` elements by style id, in document order.
    pub raw_styles: Vec<(String, String)>,
    pub doc_defaults_xml: Option<String>,
    pub latent_styles_xml: Option<String>,
    /// Top-level `w:styles` children that this model does not name.
    pub extra: Vec<String>,
}

impl StyleSheet {
    pub fn by_id(&self, id: &str) -> Option<&Style> {
        self.styles.iter().find(|s| s.id.eq_ignore_ascii_case(id))
    }

    pub fn outline_of(&self, id_or_name: &str) -> Option<i32> {
        self.styles
            .iter()
            .find(|s| {
                s.id.eq_ignore_ascii_case(id_or_name) || s.name.eq_ignore_ascii_case(id_or_name)
            })
            .and_then(|s| s.para_props.outline)
    }

    pub fn push(&mut self, style: Style) {
        if let Some(slot) = self
            .styles
            .iter_mut()
            .find(|have| have.id.eq_ignore_ascii_case(&style.id))
        {
            *slot = style;
        } else {
            self.styles.push(style);
        }
    }

    /// Replace a style, and drop its kept markup so that the writer rebuilds it.
    pub fn replace(&mut self, mut style: Style) {
        style.raw_xml = None;
        self.push(style);
    }
}

pub fn heading_level(name: &str) -> Option<i32> {
    let lower = name.to_ascii_lowercase();
    let rest = lower.strip_prefix("heading")?;
    let rest = rest.trim();
    let n: i32 = rest.parse().ok()?;
    if (1..=9).contains(&n) {
        Some(n)
    } else {
        None
    }
}
