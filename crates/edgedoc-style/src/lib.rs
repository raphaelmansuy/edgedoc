//! The style cascade, numbering, and computed properties.
//!
//! The cascade resolves once per document into [`ComputedPara`] and [`ComputedRun`] values, so the
//! flow crate never reads a [`ParaProps`] or a style sheet. The order is fixed by `DP-S-02`:
//! document defaults, table style, numbering level, paragraph style chain, direct paragraph
//! formatting, character style chain, direct run formatting.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::BTreeMap;

use edgedoc_geom::{twips, Rgba};
use edgedoc_model::{
    CharProps, ParaProps, Paragraph, Style, StyleKind, StyleSheet, TabStop as ModelTabStop,
};

use edgedoc_text::{Align, TabAlign};

/// The longest style chain that the cascade follows before it stops. See `DP-S-02`.
pub const MAX_STYLE_CHAIN: usize = 32;

/// A computed paragraph property set. It holds numbers and small enums, so a fragment can clone
/// it cheaply. See `DP-S-09`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedPara {
    pub style_id: String,
    pub outline_level: Option<i32>,
    pub align: Align,
    pub bidi: bool,
    pub kinsoku: bool,
    pub word_wrap: bool,
    pub indent_left: f32,
    pub indent_right: f32,
    pub indent_first: f32,
    pub space_before: f32,
    pub space_after: f32,
    pub space_before_auto: bool,
    pub space_after_auto: bool,
    /// A supported margin-aligned footer field is positioned outside story flow.
    pub footer_page_frame: bool,
    /// Registered face for a numbering label, when its requested face is available.
    pub label_face: Option<u32>,
    pub label_fallback_face: u32,
    pub label_size: Option<f32>,
    /// Whether the resolved list label enables pair kerning.
    pub label_kerning: bool,
    pub line_height: f32,
    /// True when `line_height` is a fixed value that clips.
    pub line_exact: bool,
    /// True when `line_height` is a minimum.
    pub line_at_least: bool,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub page_break_before: bool,
    pub widow_control: bool,
    pub contextual_spacing: bool,
    pub tabs: Vec<edgedoc_text::TabStop>,
    pub default_tab: f32,
    pub shading: Option<Rgba>,
    pub borders: ParagraphBorders,
    pub list: Option<ListRef>,
    pub snap_to_grid: bool,
}

impl Default for ComputedPara {
    fn default() -> Self {
        Self {
            style_id: String::new(),
            outline_level: None,
            align: Align::Left,
            bidi: false,
            kinsoku: true,
            word_wrap: true,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first: 0.0,
            space_before: 0.0,
            space_after: 0.0,
            space_before_auto: false,
            space_after_auto: false,
            footer_page_frame: false,
            label_face: None,
            label_fallback_face: 0,
            label_size: None,
            label_kerning: false,
            line_height: 0.0,
            line_exact: false,
            line_at_least: false,
            keep_next: false,
            keep_lines: false,
            page_break_before: false,
            widow_control: true,
            contextual_spacing: false,
            tabs: Vec::new(),
            default_tab: 36.0,
            shading: None,
            borders: ParagraphBorders::default(),
            list: None,
            snap_to_grid: true,
        }
    }
}

/// A paragraph border set, in points.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ParagraphBorders {
    pub top: Option<Border>,
    pub bottom: Option<Border>,
    pub left: Option<Border>,
    pub right: Option<Border>,
}

/// One border line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub width: f32,
    pub color: Rgba,
    pub style: BorderStyle,
    pub space: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Single,
    Double,
    Dotted,
    Dashed,
    Thick,
    Wave,
}

impl BorderStyle {
    pub fn parse(text: &str) -> Self {
        match text {
            "none" | "nil" => BorderStyle::None,
            "double" => BorderStyle::Double,
            "dotted" => BorderStyle::Dotted,
            "dashed" | "dashSmallGap" => BorderStyle::Dashed,
            "thick" => BorderStyle::Thick,
            "wave" | "doubleWave" => BorderStyle::Wave,
            _ => BorderStyle::Single,
        }
    }

    /// True when the style paints a line.
    pub fn is_visible(self) -> bool {
        !matches!(self, BorderStyle::None)
    }
}

/// A reference to a numbering level. See `DP-S-14`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListRef {
    pub num_id: u32,
    /// The abstract definition whose counters are shared by its list instances.
    pub abstract_id: u32,
    pub level: u32,
    /// The level text, such as `%1.` or a bullet character.
    pub text: String,
    /// The number format, such as `decimal` or `lowerLetter`.
    pub format: String,
    /// Formats of referenced levels in a multilevel label.
    pub level_formats: Vec<String>,
    pub start: u32,
    /// A list-instance restart, applied once when this level is first used.
    pub start_override: Option<u32>,
    /// List-level tab stop, measured from the column margin.
    pub tab: Option<f32>,
    /// How the level separates its label from the text.
    pub suffix: String,
    pub justification: String,
    pub font: Option<String>,
    /// True when every level counts as a decimal number.
    pub is_legal: bool,
    /// Whether the level's run formatting displays its label in uppercase.
    pub caps: bool,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub size: Option<f32>,
}

/// A computed run property set.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedRun {
    /// The face that the document asked for.
    pub requested_face: String,
    /// The face that the database resolved.
    pub face_name: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub double_underline: bool,
    pub strike: bool,
    pub double_strike: bool,
    pub size: f32,
    /// Minimum font size that permits kerning, in points.
    pub kern: Option<f32>,
    pub color: Rgba,
    pub highlight: Option<Rgba>,
    pub shading: Option<Rgba>,
    pub vert_align: Option<VertAlign>,
    pub caps: edgedoc_text::CapsMode,
    pub hidden: bool,
    pub rtl: bool,
    pub letter_spacing: f32,
    pub baseline_shift: f32,
    pub x_scale: f32,
    pub lang: Option<String>,
    pub underline_color: Option<Rgba>,
    pub emboss: bool,
    pub imprint: bool,
    pub outline: bool,
    pub shadow: bool,
    /// The symbol font that the run names, when it names one. Its 8-bit codes map to Unicode, so
    /// the symbol paints from an installed face. See `DP-F-35`.
    pub symbol: Option<String>,
    /// True when the document asks for bold and the face has no bold sibling, so the painter
    /// emboldens the glyphs. See `DP-F-13`.
    pub synth_bold: bool,
    /// True when the document asks for italic and the face has no italic sibling.
    pub synth_italic: bool,
}

/// Vertical position of a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VertAlign {
    Superscript,
    Subscript,
}

impl Default for ComputedRun {
    fn default() -> Self {
        Self {
            requested_face: String::new(),
            face_name: String::new(),
            bold: false,
            italic: false,
            underline: false,
            double_underline: false,
            strike: false,
            double_strike: false,
            size: 11.0,
            kern: None,
            color: Rgba::BLACK,
            highlight: None,
            shading: None,
            vert_align: None,
            caps: edgedoc_text::CapsMode::None,
            hidden: false,
            rtl: false,
            letter_spacing: 0.0,
            baseline_shift: 0.0,
            x_scale: 1.0,
            lang: None,
            underline_color: None,
            emboss: false,
            imprint: false,
            outline: false,
            shadow: false,
            symbol: None,
            synth_bold: false,
            synth_italic: false,
        }
    }
}

/// One header or footer slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeaderSlot {
    #[default]
    Default,
    First,
    Even,
}

/// A diagnostic from the style stage.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleDiagnostic {
    pub code: &'static str,
    pub detail: String,
}

/// The style resolver. It holds the sheet, the defaults, and the numbering table.
#[derive(Debug, Clone)]
pub struct Cascade {
    compatibility_mode: u32,
    default_style: Option<String>,
    theme_fonts: BTreeMap<String, String>,
    sheet: StyleSheet,
    /// Document defaults from `w:docDefaults`.
    default_run: ComputedRun,
    default_para: ComputedPara,
    /// The default tab interval from `w:defaultTabStop`.
    default_tab: f32,
    /// Numbering definitions by `numId`.
    numbering: BTreeMap<u32, NumDef>,
    /// Families that the font database knows. A face outside this set is reported once.
    known_faces: Vec<String>,
    pub diagnostics: Vec<StyleDiagnostic>,
}

/// One numbering definition.
#[derive(Debug, Clone, Default)]
pub struct NumDef {
    pub abstract_id: u32,
    pub levels: BTreeMap<u32, NumLevel>,
    pub start_overrides: BTreeMap<u32, u32>,
}

/// One level of a numbering definition.
#[derive(Debug, Clone, Default)]
pub struct NumLevel {
    pub format: String,
    /// The level text, such as `%1.` or `\u{2022}`.
    pub text: String,
    pub start: u32,
    pub indent_left: f32,
    pub indent_hanging: f32,
    pub tab: Option<f32>,
    pub font: Option<String>,
    pub suffix: String,
    pub justification: String,
    pub is_legal: bool,
    pub caps: bool,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub size: Option<f32>,
}

impl NumLevel {
    /// The bullet character of a bullet level, or `None`.
    pub fn bullet(&self) -> Option<char> {
        if self.format == "bullet" {
            self.text.chars().find(|c| !c.is_whitespace())
        } else {
            None
        }
    }
}

impl Cascade {
    /// Register a built-in fallback only when a document did not define it.
    pub fn add_fallback_style(&mut self, style: Style) {
        if self.sheet.by_id(&style.id).is_none() {
            self.sheet.styles.push(style);
        }
    }

    pub fn new(sheet: &StyleSheet, known_faces: &[String]) -> Self {
        let default_run = ComputedRun {
            size: 11.0,
            face_name: "Carlito".to_string(),
            requested_face: "Calibri".to_string(),
            ..ComputedRun::default()
        };
        let default_para = ComputedPara::default();
        Self {
            compatibility_mode: 15,
            theme_fonts: BTreeMap::new(),
            default_style: sheet
                .by_id("Normal")
                .filter(|style| style.kind == StyleKind::Paragraph)
                .map(|style| style.id.clone()),
            sheet: sheet.clone(),
            default_run,
            default_para,
            default_tab: 36.0,
            numbering: BTreeMap::new(),
            known_faces: known_faces.to_vec(),
            diagnostics: Vec::new(),
        }
    }

    /// Select the default paragraph style, including a localized style ID.
    pub fn set_default_style(&mut self, id: &str) {
        if self
            .sheet
            .by_id(id)
            .is_some_and(|style| style.kind == StyleKind::Paragraph)
        {
            self.default_style = Some(id.to_string());
        }
    }

    pub fn set_theme_font(&mut self, slot: &str, family: &str) {
        if !family.is_empty() {
            self.theme_fonts
                .insert(slot.to_string(), family.to_string());
        }
    }

    /// Set the document compatibility mode used for table placement.
    pub fn set_compatibility_mode(&mut self, mode: u32) {
        self.compatibility_mode = mode;
    }

    pub fn compatibility_mode(&self) -> u32 {
        self.compatibility_mode
    }

    /// Set the default tab interval, in points.
    pub fn set_default_tab(&mut self, pt: f32) {
        if pt > 0.0 && pt.is_finite() {
            self.default_tab = pt;
        }
    }

    /// Apply decoded document defaults before style and direct formatting.
    pub fn set_default_properties(&mut self, run: &CharProps, para: &ParaProps) {
        apply_run(&mut self.default_run, run);
        apply_para(&mut self.default_para, para);
    }

    pub fn set_default_run(&mut self, run: ComputedRun) {
        self.default_run = run;
    }

    /// Add a numbering definition.
    pub fn add_numbering(&mut self, num_id: u32, def: NumDef) {
        self.numbering.insert(num_id, def);
    }

    pub fn numbering(&self) -> &BTreeMap<u32, NumDef> {
        &self.numbering
    }

    /// Resolve the paragraph style chain, from the root to the named style. See `DP-S-02`.
    fn chain(&mut self, style_id: &str) -> Vec<Style> {
        let mut out: Vec<Style> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut current = Some(style_id.to_string());
        let mut steps = 0;
        while let Some(id) = current {
            steps += 1;
            if steps > MAX_STYLE_CHAIN {
                self.diagnostics.push(StyleDiagnostic {
                    code: "display.style_chain_too_long",
                    detail: format!("\"{style_id}\" follows more than {MAX_STYLE_CHAIN} styles"),
                });
                break;
            }
            if seen.iter().any(|s| s == &id) {
                self.diagnostics.push(StyleDiagnostic {
                    code: "display.style_cycle",
                    detail: format!("\"{id}\" is its own ancestor"),
                });
                break;
            }
            seen.push(id.clone());
            let Some(style) = self.sheet.by_id(&id) else {
                if !id.is_empty() {
                    self.diagnostics.push(StyleDiagnostic {
                        code: "display.style_undefined",
                        detail: format!("style \"{id}\" has no definition"),
                    });
                }
                break;
            };
            if style.kind != StyleKind::Paragraph && style.kind != StyleKind::Character {
                break;
            }
            current = style.based_on.clone();
            out.push(style.clone());
        }
        out.reverse();
        out
    }

    /// Resolve a paragraph. See `DP-S-01` and `DP-S-08`.
    pub fn paragraph(&mut self, para: &Paragraph) -> ComputedPara {
        let mut out = self.default_para.clone();
        out.default_tab = self.default_tab;
        let style_id = para
            .props
            .style
            .clone()
            .or_else(|| self.default_style.clone())
            .unwrap_or_default();
        out.style_id = style_id.clone();
        // 4. Paragraph style chain.
        let mut style_num_id = None;
        let mut style_level = None;
        let mut style_list_indent_left = false;
        let mut style_list_indent_first = false;
        if !style_id.is_empty() {
            for style in self.chain(&style_id) {
                if style.kind == StyleKind::Paragraph {
                    if style.para_props.num_id.is_some() {
                        style_num_id = style.para_props.num_id;
                        style_level = style.para_props.ilvl;
                        style_list_indent_left = false;
                        style_list_indent_first = false;
                    } else if style.para_props.ilvl.is_some() {
                        style_level = style.para_props.ilvl;
                    }
                    if style_num_id.is_some() {
                        style_list_indent_left |= style.para_props.indent_left.is_some()
                            || style.para_props.indent_left_chars.is_some_and(|v| v != 0);
                        style_list_indent_first |= style.para_props.indent_first.is_some()
                            || style.para_props.indent_first_chars.is_some_and(|v| v != 0);
                    }
                    apply_para(&mut out, &style.para_props);
                    if let Some(outline) = style.para_props.outline {
                        out.outline_level = Some(outline);
                    }
                }
            }
        }
        // 3. Numbering level, before the direct paragraph formatting.
        let direct_list = para.props.num_id.is_some();
        let list_id = para.props.num_id.or(style_num_id);
        let list_level = para
            .props
            .ilvl
            .or(if direct_list { None } else { style_level })
            .unwrap_or(0);
        if let Some(num_id) = list_id.filter(|id| *id != 0) {
            let level = list_level;
            if let Some(def) = self.numbering.get(&num_id) {
                if let Some(lvl) = def.levels.get(&level) {
                    if lvl.indent_left > 0.0 && (direct_list || !style_list_indent_left) {
                        out.indent_left = lvl.indent_left;
                    }
                    if lvl.indent_hanging != 0.0 && (direct_list || !style_list_indent_first) {
                        out.indent_first = -lvl.indent_hanging;
                    }
                }
            }
            let (text, format, start, is_legal, suffix, justification, font) = self
                .numbering
                .get(&num_id)
                .and_then(|d| d.levels.get(&level.min(8)))
                .map(|l| {
                    let text = l.text.clone();
                    (
                        text,
                        l.format.clone(),
                        l.start,
                        l.is_legal,
                        l.suffix.clone(),
                        l.justification.clone(),
                        l.font.clone(),
                    )
                })
                .unwrap_or_default();
            out.list = Some(ListRef {
                num_id,
                abstract_id: self
                    .numbering
                    .get(&num_id)
                    .map_or(num_id, |d| d.abstract_id),
                level: level.min(8),
                text,
                format,
                level_formats: self
                    .numbering
                    .get(&num_id)
                    .map(|d| {
                        (0..9)
                            .map(|i| {
                                d.levels
                                    .get(&i)
                                    .map_or_else(|| "decimal".to_string(), |l| l.format.clone())
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                start,
                start_override: self
                    .numbering
                    .get(&num_id)
                    .and_then(|d| d.start_overrides.get(&level.min(8)).copied()),
                tab: self
                    .numbering
                    .get(&num_id)
                    .and_then(|d| d.levels.get(&level.min(8)))
                    .and_then(|l| l.tab),
                is_legal,
                caps: self
                    .numbering
                    .get(&num_id)
                    .and_then(|d| d.levels.get(&level.min(8)))
                    .is_some_and(|l| l.caps),
                bold: self
                    .numbering
                    .get(&num_id)
                    .and_then(|d| d.levels.get(&level.min(8)))
                    .and_then(|l| l.bold),
                italic: self
                    .numbering
                    .get(&num_id)
                    .and_then(|d| d.levels.get(&level.min(8)))
                    .and_then(|l| l.italic),
                size: self
                    .numbering
                    .get(&num_id)
                    .and_then(|d| d.levels.get(&level.min(8)))
                    .and_then(|l| l.size),
                suffix,
                justification,
                font,
            });
        } else if list_id.is_none() {
            if let Some(spec) = para.props.list {
                // A list that the builder named directly.
                out.list = Some(ListRef {
                    num_id: spec.level + 1000,
                    abstract_id: spec.level + 1000,
                    level: spec.level,
                    text: spec.kind.level_text(spec.level),
                    format: spec.kind.num_fmt().to_string(),
                    level_formats: Vec::new(),
                    start: 1,
                    start_override: None,
                    tab: None,
                    is_legal: false,
                    caps: false,
                    bold: None,
                    italic: None,
                    size: None,
                    suffix: "tab".into(),
                    justification: "left".into(),
                    font: None,
                });
            }
        }
        // 6. Direct paragraph formatting.
        apply_para(&mut out, &para.props);
        if let Some(outline) = para.props.outline {
            out.outline_level = Some(outline);
        }
        if out.space_before_auto {
            out.space_before = 14.0;
        }
        if out.space_after_auto {
            out.space_after = 14.0;
        }
        // A built-in heading style carries its level even with no `w:outlineLvl`. See ED-X-16.
        if out.outline_level.is_none() {
            out.outline_level = self.sheet.outline_of(&style_id);
        }
        out
    }

    /// Resolve a table style and its ancestors, from the root to the named style.
    pub fn table_style_chain(&mut self, id: &str) -> Vec<Style> {
        let mut chain = Vec::new();
        let mut seen = Vec::new();
        let mut next = Some(id.to_string());
        while let Some(id) = next {
            if seen.len() >= MAX_STYLE_CHAIN || seen.contains(&id) {
                break;
            }
            seen.push(id.clone());
            let Some(style) = self.sheet.by_id(&id) else {
                break;
            };
            if style.kind != StyleKind::Table {
                break;
            }
            next = style.based_on.clone();
            chain.push(style.clone());
        }
        chain.reverse();
        chain
    }

    /// Put table-style text properties between document defaults and paragraph styles.
    pub fn apply_table_style_defaults(&mut self, chain: &[Style]) {
        for style in chain {
            apply_para(&mut self.default_para, &style.para_props);
            apply_run(&mut self.default_run, &style.char_props);
        }
    }

    /// Save and restore the table text defaults while resolving individual cells.
    pub fn table_defaults(&self) -> (ComputedPara, ComputedRun) {
        (self.default_para.clone(), self.default_run.clone())
    }

    pub fn restore_table_defaults(&mut self, defaults: (ComputedPara, ComputedRun)) {
        (self.default_para, self.default_run) = defaults;
    }

    /// Apply a conditional table region before paragraph and direct formatting.
    pub fn apply_table_region_defaults(&mut self, region: &edgedoc_model::TableStyleRegion) {
        apply_para(&mut self.default_para, &region.para_props);
        apply_run(&mut self.default_run, &region.char_props);
    }

    /// Resolve one run of a paragraph.
    pub fn run(&mut self, para: &Paragraph, run: &CharProps) -> ComputedRun {
        let mut out = self.default_run.clone();
        out.x_scale = 1.0;
        // 4. Paragraph style chain: its character properties are the base for the run.
        if let Some(style_id) = para
            .props
            .style
            .as_ref()
            .or(self.default_style.as_ref())
            .cloned()
        {
            for style in self.chain(&style_id) {
                if style.kind == StyleKind::Paragraph {
                    apply_run(&mut out, &style.char_props);
                }
            }
        }
        // 7. Character style chain. A character style sets character properties only, so a
        // paragraph style in the same chain is skipped. See `DP-S-02`.
        if let Some(style_id) = &run.style {
            for style in self.chain(style_id) {
                if style.kind == StyleKind::Character || style.kind == StyleKind::Paragraph {
                    apply_run(&mut out, &style.char_props);
                }
            }
        }
        // 8. Direct run formatting.
        apply_run(&mut out, run);
        if let Some(family) = self.theme_fonts.get(&out.face_name) {
            out.face_name = family.clone();
            out.requested_face = family.clone();
        } else if matches!(out.face_name.as_str(), "+mj-lt" | "+mn-lt") {
            out.face_name = "Calibri".into();
            out.requested_face = "Calibri".into();
        }
        if out.requested_face.is_empty() {
            out.requested_face = "Calibri".to_string();
        }
        if out.face_name.is_empty() {
            out.face_name = out.requested_face.clone();
        }
        // A symbol font maps its codes instead of painting itself. See `DP-F-35`.
        if edgedoc_fonts_symbol_is_symbol(&out.requested_face) {
            out.symbol = Some(out.requested_face.clone());
        }
        // Report a face that the database does not hold, once per name. See DP-F-03.
        let known = self
            .known_faces
            .iter()
            .any(|f| f.eq_ignore_ascii_case(&out.face_name));
        if !known
            && !self
                .diagnostics
                .iter()
                .any(|d| d.code == "display.font_substituted" && d.detail == out.face_name)
        {
            self.diagnostics.push(StyleDiagnostic {
                code: "display.font_substituted",
                detail: out.face_name.clone(),
            });
        }
        out
    }
}

/// The size that a character indent is measured in, which is the paragraph mark's size.
fn out_mark_size(_out: &ComputedPara) -> f32 {
    // The mark takes the style's size, which the run cascade applies. Version 1 reads the mark's
    // own size, and falls back to 11 pt, which is Word's default body size.
    11.0
}

fn apply_para(out: &mut ComputedPara, props: &ParaProps) {
    if let Some(kinsoku) = props.kinsoku {
        out.kinsoku = kinsoku;
    }
    if let Some(word_wrap) = props.word_wrap {
        out.word_wrap = word_wrap;
    }
    if let Some(align) = props.align.as_deref().and_then(Align::parse) {
        out.align = align;
    }
    if props.bidi {
        out.bidi = true;
    }
    if let Some(v) = props.indent_left {
        out.indent_left = twips(v.0);
    }
    if let Some(v) = props.indent_right {
        out.indent_right = twips(v.0);
    }
    if let Some(v) = props.indent_first {
        out.indent_first = twips(v.0);
    }
    // OOXML stores character indents in hundredths of a character. See `DP-S-31`.
    let mark_size = props.mark.size_pt.unwrap_or(out_mark_size(out));
    if let Some(chars) = props.indent_left_chars {
        if chars != 0 {
            out.indent_left = chars as f32 / 100.0 * mark_size;
        }
    }
    if let Some(chars) = props.indent_right_chars {
        if chars != 0 {
            out.indent_right = chars as f32 / 100.0 * mark_size;
        }
    }
    if let Some(chars) = props.indent_first_chars {
        if chars != 0 {
            out.indent_first = chars as f32 / 100.0 * mark_size;
        }
    }
    if let Some(value) = props.spacing_before_auto {
        out.space_before_auto = value;
    }
    if let Some(value) = props.spacing_after_auto {
        out.space_after_auto = value;
    }
    if let Some(v) = props.spacing_before {
        out.space_before = twips(v.0);
    }
    if let Some(v) = props.spacing_after {
        out.space_after = twips(v.0);
    }
    if let Some(v) = props.line {
        out.line_height = twips(v.0);
    }
    if props.line.is_some() || props.line_rule.is_some() {
        match props.line_rule.as_deref() {
            Some("exact") => {
                out.line_exact = true;
                out.line_at_least = false;
            }
            Some("atLeast") => {
                out.line_at_least = true;
                out.line_exact = false;
            }
            _ => {
                out.line_exact = false;
                out.line_at_least = false;
            }
        }
    }
    let explicit = |name: &str| props.explicit_on_off.iter().any(|item| item == name);
    if props.keep_next || explicit("keepNext") {
        out.keep_next = props.keep_next;
    }
    if props.keep_lines || explicit("keepLines") {
        out.keep_lines = props.keep_lines;
    }
    if props.page_break_before || explicit("pageBreakBefore") {
        out.page_break_before = props.page_break_before;
    }
    if !props.widow_control || explicit("widowControl") {
        out.widow_control = props.widow_control;
    }
    if props.contextual_spacing || explicit("contextualSpacing") {
        out.contextual_spacing = props.contextual_spacing;
    }
    if let Some(shading) = props.shading.as_deref().and_then(Rgba::parse_hex) {
        out.shading = Some(shading);
    }
    if !props.tabs.is_empty() {
        for tab in &props.tabs {
            let position = twips(tab.pos.0);
            out.tabs
                .retain(|existing| (existing.pos - position).abs() > 0.01);
            if tab.val != "clear" {
                out.tabs.push(tab_from_model(tab));
            }
        }
    }
    // An explicit paragraph-border set replaces the inherited set as a whole.
    // An omitted set leaves the inherited borders intact.
    if props.borders_present || !props.borders.is_empty() {
        out.borders = borders_from_model(&props.borders);
    }
    if let Some(snap) = props.mark.snap_to_grid {
        out.snap_to_grid = snap;
    }
}

fn tab_from_model(tab: &ModelTabStop) -> edgedoc_text::TabStop {
    edgedoc_text::TabStop {
        pos: twips(tab.pos.0),
        align: TabAlign::parse(&tab.val),
        leader: tab.leader.as_deref().and_then(|l| match l {
            "dot" => Some('.'),
            "hyphen" => Some('-'),
            "underscore" => Some('_'),
            "middleDot" => Some('\u{00B7}'),
            _ => None,
        }),
    }
}

fn borders_from_model(b: &edgedoc_model::Borders) -> ParagraphBorders {
    let convert = |l: &Option<edgedoc_model::LineBorder>| {
        l.as_ref().map(|line| Border {
            width: line.size.map_or(0.5, |s| s as f32 / 8.0),
            color: line
                .color
                .as_deref()
                .and_then(Rgba::parse_hex)
                .unwrap_or(Rgba::BLACK),
            style: BorderStyle::parse(&line.val),
            space: line.space.map_or(0.0, |s| s as f32),
        })
    };
    ParagraphBorders {
        top: convert(&b.top),
        bottom: convert(&b.bottom),
        left: convert(&b.left),
        right: convert(&b.right),
    }
}

fn apply_run(out: &mut ComputedRun, props: &CharProps) {
    let off = |name: &str| props.explicit_off.iter().any(|value| value == name);
    if off("b") {
        out.bold = false;
    }
    if off("i") {
        out.italic = false;
    }
    if off("strike") {
        out.strike = false;
    }
    if off("dstrike") {
        out.double_strike = false;
    }
    if off("vanish") {
        out.hidden = false;
    }
    if off("rtl") {
        out.rtl = false;
    }
    if off("emboss") {
        out.emboss = false;
    }
    if off("imprint") {
        out.imprint = false;
    }
    if off("outline") {
        out.outline = false;
    }
    if off("shadow") {
        out.shadow = false;
    }
    if (off("caps") && out.caps == edgedoc_text::CapsMode::All)
        || (off("smallCaps") && out.caps == edgedoc_text::CapsMode::Small)
    {
        out.caps = edgedoc_text::CapsMode::None;
    }
    if let Some(value) = props.underline_val.as_deref() {
        out.double_underline = value == "double";
    }
    if props
        .underline_val
        .as_deref()
        .is_some_and(|value| matches!(value, "none" | "0" | "false" | "off"))
    {
        out.underline = false;
    }
    if let Some(threshold) = props.kern {
        out.kern = Some(threshold as f32 / 2.0);
    }
    if props.bold {
        out.bold = true;
    }
    if props.italic {
        out.italic = true;
    }
    if props.underline {
        out.underline = true;
    }
    if props.strike {
        out.strike = true;
    }
    if props.double_strike {
        out.double_strike = true;
    }
    if let Some(size) = props.size_pt {
        if size > 0.0 && size < 1638.0 {
            out.size = size;
        }
    }
    if let Some(face) = props.font.as_ref().or(props.font_hansi.as_ref()) {
        out.requested_face = face.clone();
        out.face_name = face.clone();
    }
    if let Some(color) = props.color.as_deref().and_then(Rgba::parse_hex) {
        out.color = color;
    }
    if let Some(highlight) = props.highlight.as_deref().and_then(highlight_color) {
        out.highlight = Some(highlight);
    }
    if let Some(shading) = props.shading.as_deref().and_then(Rgba::parse_hex) {
        out.shading = Some(shading);
    }
    match props.vert_align.as_deref() {
        Some("superscript") => out.vert_align = Some(VertAlign::Superscript),
        Some("subscript") => out.vert_align = Some(VertAlign::Subscript),
        _ => {}
    }
    if props.caps {
        out.caps = edgedoc_text::CapsMode::All;
    } else if props.small_caps {
        out.caps = edgedoc_text::CapsMode::Small;
    }
    if props.vanish {
        out.hidden = true;
    }
    if props.rtl {
        out.rtl = true;
    }
    if let Some(spacing) = props.spacing {
        out.letter_spacing = spacing as f32 / 20.0;
    }
    if let Some(position) = props.position {
        out.baseline_shift = position as f32 / 2.0;
    }
    if let Some(scale) = props.scale {
        if scale > 0 && scale <= 600 {
            out.x_scale = scale as f32 / 100.0;
        }
    }
    if let Some(lang) = &props.lang {
        out.lang = Some(lang.clone());
    }
    if let Some(color) = props.underline_color.as_deref().and_then(Rgba::parse_hex) {
        out.underline_color = Some(color);
    }
    if props.emboss {
        out.emboss = true;
    }
    if props.imprint {
        out.imprint = true;
    }
    if props.outline {
        out.outline = true;
    }
    if props.shadow {
        out.shadow = true;
    }
}

/// A highlight name to a colour, from the Word palette.
pub fn highlight_color(name: &str) -> Option<Rgba> {
    let c = match name {
        "yellow" => Rgba::opaque(0xFF, 0xFF, 0x00),
        "green" => Rgba::opaque(0x00, 0xFF, 0x00),
        "cyan" => Rgba::opaque(0x00, 0xFF, 0xFF),
        "magenta" => Rgba::opaque(0xFF, 0x00, 0xFF),
        "blue" => Rgba::opaque(0x00, 0x00, 0xFF),
        "red" => Rgba::opaque(0xFF, 0x00, 0x00),
        "darkBlue" => Rgba::opaque(0x00, 0x00, 0x80),
        "darkCyan" => Rgba::opaque(0x00, 0x80, 0x80),
        "darkGreen" => Rgba::opaque(0x00, 0x80, 0x00),
        "darkMagenta" => Rgba::opaque(0x80, 0x00, 0x80),
        "darkRed" => Rgba::opaque(0x80, 0x00, 0x00),
        "darkYellow" => Rgba::opaque(0x80, 0x80, 0x00),
        "darkGray" => Rgba::opaque(0x80, 0x80, 0x80),
        "lightGray" => Rgba::opaque(0xC0, 0xC0, 0xC0),
        "black" => Rgba::opaque(0x00, 0x00, 0x00),
        "white" => Rgba::WHITE,
        "none" => return None,
        _ => return None,
    };
    Some(c)
}

/// Format a counter with a numbering format. See `DP-S-21`.
pub fn format_counter(value: u32, format: &str, _is_legal: bool) -> String {
    let v = value;
    match format {
        "decimal" | "decimalHalfWidth" => v.to_string(),
        "decimalZero" => {
            if v < 10 {
                format!("0{v}")
            } else {
                v.to_string()
            }
        }
        "upperRoman" => roman(v).to_uppercase(),
        "lowerRoman" => roman(v),
        "upperLetter" => letter(v).to_uppercase(),
        "lowerLetter" => letter(v),
        "ordinal" => format!("{v}{}", ordinal_suffix(v)),
        "cardinalText" => cardinal_text(v),
        "ordinalText" => ordinal_text(v),
        "bullet" | "none" => String::new(),
        "decimalFullWidth" => v
            .to_string()
            .chars()
            .map(|c| char::from_u32(0xFF10 + u32::from(c) - u32::from('0')).unwrap_or(c))
            .collect(),
        _ => v.to_string(),
    }
}

fn roman(mut value: u32) -> String {
    let table = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (n, s) in table {
        while value >= n {
            out.push_str(s);
            value -= n;
        }
    }
    out
}

fn letter(mut value: u32) -> String {
    if value == 0 {
        return String::new();
    }
    let base = b'a';
    let mut out = String::new();
    // 1 is `a`, 26 is `z`, 27 is `aa`, which is the Word rule.
    while value > 0 {
        let v = ((value - 1) % 26) as u8;
        out.insert(0, char::from(base + v));
        value = (value - 1) / 26;
    }
    out
}

fn ordinal_suffix(v: u32) -> &'static str {
    match v % 100 {
        11..=13 => "th",
        _ => match v % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    }
}

fn cardinal_text(v: u32) -> String {
    const ONES: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 8] = [
        "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    if v < 20 {
        return ONES.get(v as usize).copied().unwrap_or("").to_string();
    }
    if v < 100 {
        let t = TENS.get((v / 10 - 2) as usize).copied().unwrap_or("");
        return if v % 10 == 0 {
            t.to_string()
        } else {
            format!("{t}-{}", ONES.get((v % 10) as usize).copied().unwrap_or(""))
        };
    }
    if v < 1000 {
        let h = ONES.get((v / 100) as usize).copied().unwrap_or("");
        let rest = v % 100;
        return if rest == 0 {
            format!("{h} hundred")
        } else {
            format!("{h} hundred {}", cardinal_text(rest))
        };
    }
    v.to_string()
}

fn ordinal_text(v: u32) -> String {
    let c = cardinal_text(v);
    let replace: &[(&str, &str)] = &[
        ("one", "first"),
        ("two", "second"),
        ("three", "third"),
        ("five", "fifth"),
        ("eight", "eighth"),
        ("nine", "ninth"),
        ("twelve", "twelfth"),
        ("twenty", "twentieth"),
        ("thirty", "thirtieth"),
        ("forty", "fortieth"),
        ("fifty", "fiftieth"),
        ("sixty", "sixtieth"),
        ("seventy", "seventieth"),
        ("eighty", "eightieth"),
        ("ninety", "ninetieth"),
        ("hundred", "hundredth"),
    ];
    let mut out = c;
    for (from, to) in replace {
        if out.ends_with(from) {
            let cut = out.len() - from.len();
            out.truncate(cut);
            out.push_str(to);
            return out;
        }
    }
    if out.ends_with('y') {
        out.pop();
        out.push_str("ieth");
    } else {
        out.push_str("th");
    }
    out
}

/// True when a family name is one of Word's symbol fonts. The check lives in `edgedoc-fonts`,
/// which reads the tables; the style crate only asks the question.
fn edgedoc_fonts_symbol_is_symbol(family: &str) -> bool {
    const SYMBOLS: &[&str] = &[
        "symbol",
        "symbol (pcl6)",
        "wingdings",
        "wingdings 2",
        "wingdings 3",
        "webdings",
    ];
    SYMBOLS
        .iter()
        .any(|s| family.trim().eq_ignore_ascii_case(s))
}

#[cfg(test)]
// A test builds a property set field by field, because only two of twenty fields matter and a
// full struct literal would hide which. That is clearer than a builder for one assertion.
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn tab_clears_remove_inherited_stops_without_removing_other_stops() {
        let mut para = ComputedPara::default();
        let mut inherited = ParaProps::default();
        inherited.tabs = vec![
            ModelTabStop {
                val: "left".into(),
                pos: edgedoc_model::Twips(720),
                leader: None,
            },
            ModelTabStop {
                val: "right".into(),
                pos: edgedoc_model::Twips(1440),
                leader: None,
            },
        ];
        apply_para(&mut para, &inherited);
        let mut direct = ParaProps::default();
        direct.tabs = vec![
            ModelTabStop {
                val: "clear".into(),
                pos: edgedoc_model::Twips(720),
                leader: None,
            },
            ModelTabStop {
                val: "center".into(),
                pos: edgedoc_model::Twips(1080),
                leader: None,
            },
        ];
        apply_para(&mut para, &direct);
        assert_eq!(para.tabs.len(), 2);
        assert!(para
            .tabs
            .iter()
            .any(|tab| tab.pos == 54.0 && tab.align == TabAlign::Center));
        assert!(para
            .tabs
            .iter()
            .any(|tab| tab.pos == 72.0 && tab.align == TabAlign::Right));
    }

    #[test]
    fn a_roman_counter_follows_the_table() {
        assert_eq!(format_counter(1, "upperRoman", false), "I");
        assert_eq!(format_counter(4, "lowerRoman", false), "iv");
        assert_eq!(format_counter(27, "lowerRoman", false), "xxvii");
    }

    #[test]
    fn a_letter_counter_continues_past_z() {
        assert_eq!(format_counter(1, "lowerLetter", false), "a");
        assert_eq!(format_counter(26, "lowerLetter", false), "z");
        assert_eq!(format_counter(27, "lowerLetter", false), "aa");
        assert_eq!(format_counter(28, "lowerLetter", false), "ab");
        assert_eq!(format_counter(52, "lowerLetter", false), "az");
        assert_eq!(format_counter(702, "lowerLetter", false), "zz");
    }

    #[test]
    fn decimal_zero_pads_below_ten() {
        assert_eq!(format_counter(1, "decimalZero", false), "01");
        assert_eq!(format_counter(12, "decimalZero", false), "12");
    }

    #[test]
    fn an_ordinal_has_the_right_suffix() {
        assert_eq!(format_counter(1, "ordinal", false), "1st");
        assert_eq!(format_counter(2, "ordinal", false), "2nd");
        assert_eq!(format_counter(3, "ordinal", false), "3rd");
        assert_eq!(format_counter(11, "ordinal", false), "11th");
        assert_eq!(format_counter(27, "ordinal", false), "27th");
    }

    #[test]
    fn cardinal_and_ordinal_text_read_as_words() {
        assert_eq!(format_counter(1, "cardinalText", false), "one");
        assert_eq!(format_counter(27, "cardinalText", false), "twenty-seven");
        assert_eq!(format_counter(1, "ordinalText", false), "first");
        assert_eq!(format_counter(2, "ordinalText", false), "second");
    }

    #[test]
    fn an_unknown_format_paints_a_decimal_number() {
        assert_eq!(format_counter(7, "noSuchFormat", false), "7");
    }

    #[test]
    fn a_highlight_name_maps_to_a_colour() {
        assert_eq!(
            highlight_color("yellow"),
            Some(Rgba::opaque(0xFF, 0xFF, 0x00))
        );
        assert_eq!(highlight_color("none"), None);
        assert_eq!(highlight_color("noSuch"), None);
    }

    #[test]
    fn a_cascade_reports_a_missing_style_once() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new().style("NoSuchStyle");
        let _ = cascade.paragraph(&para);
        let count = cascade
            .diagnostics
            .iter()
            .filter(|d| d.code == "display.style_undefined")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn paragraph_on_off_settings_can_override_inherited_values() {
        let mut sheet = StyleSheet::default();
        let mut style = Style::paragraph("ParagraphControls");
        style.para_props.keep_next = true;
        style.para_props.keep_lines = true;
        style.para_props.page_break_before = true;
        style.para_props.contextual_spacing = true;
        style.para_props.widow_control = false;
        sheet.push(style);
        let mut cascade = Cascade::new(&sheet, &[]);
        let mut para = Paragraph::new().style("ParagraphControls");
        let inherited = cascade.paragraph(&para);
        assert!(
            inherited.keep_next
                && inherited.keep_lines
                && inherited.page_break_before
                && inherited.contextual_spacing
        );
        assert!(!inherited.widow_control);
        para.props.explicit_on_off = [
            "keepNext",
            "keepLines",
            "pageBreakBefore",
            "contextualSpacing",
            "widowControl",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let overridden = cascade.paragraph(&para);
        assert!(
            !overridden.keep_next
                && !overridden.keep_lines
                && !overridden.page_break_before
                && !overridden.contextual_spacing
        );
        assert!(overridden.widow_control);
    }

    #[test]
    fn paragraph_wrap_options_inherit_and_allow_direct_overrides() {
        let mut sheet = StyleSheet::default();
        let mut style = Style::paragraph("WrapStyle");
        style.para_props.word_wrap = Some(false);
        style.para_props.kinsoku = Some(false);
        sheet.push(style);
        let mut cascade = Cascade::new(&sheet, &[]);
        let mut para = Paragraph::new().style("WrapStyle");
        let inherited = cascade.paragraph(&para);
        assert!(!inherited.word_wrap && !inherited.kinsoku);
        para.props.word_wrap = Some(true);
        para.props.kinsoku = Some(true);
        let overridden = cascade.paragraph(&para);
        assert!(overridden.word_wrap && overridden.kinsoku);
    }

    #[test]
    fn a_style_chain_sets_a_property() {
        let mut sheet = StyleSheet::default();
        let mut style = Style::paragraph("Heading1");
        style.para_props.outline = Some(0);
        style.para_props.keep_next = true;
        sheet.push(style);
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new().style("Heading1");
        let computed = cascade.paragraph(&para);
        assert!(computed.keep_next);
        assert_eq!(computed.outline_level, Some(0));
    }

    #[test]
    fn paragraph_style_inherits_and_can_disable_numbering() {
        let mut sheet = StyleSheet::default();
        let mut base = Style::paragraph("Numbered");
        base.para_props.num_id = Some(5);
        base.para_props.ilvl = Some(1);
        base.para_props.indent_left = Some(edgedoc_model::Twips(720));
        sheet.push(base);
        let mut derived = Style::paragraph("Derived");
        derived.based_on = Some("Numbered".into());
        sheet.push(derived);
        let mut cascade = Cascade::new(&sheet, &[]);
        let mut def = NumDef {
            abstract_id: 3,
            ..Default::default()
        };
        def.levels.insert(
            1,
            NumLevel {
                indent_left: 18.0,
                format: "decimalZero".into(),
                ..Default::default()
            },
        );
        cascade.add_numbering(5, def);
        let mut paragraph = Paragraph::new().style("Derived");
        let computed = cascade.paragraph(&paragraph);
        let list = computed.list.unwrap();
        assert_eq!((list.num_id, list.abstract_id, list.level), (5, 3, 1));
        assert_eq!(computed.indent_left, 36.0);
        paragraph.props.num_id = Some(0);
        assert!(cascade.paragraph(&paragraph).list.is_none());
    }

    #[test]
    fn direct_formatting_wins_over_a_style() {
        let mut sheet = StyleSheet::default();
        let mut style = Style::paragraph("Indented");
        style.para_props.indent_left = Some(edgedoc_model::Twips(720));
        sheet.push(style);
        let mut cascade = Cascade::new(&sheet, &[]);
        let mut para = Paragraph::new().style("Indented");
        para.props.indent_left = Some(edgedoc_model::Twips(1440));
        let computed = cascade.paragraph(&para);
        assert!((computed.indent_left - 72.0).abs() < 0.01);
    }

    #[test]
    fn a_style_cycle_stops() {
        let mut sheet = StyleSheet::default();
        let mut a = Style::paragraph("A");
        a.based_on = Some("B".into());
        let mut b = Style::paragraph("B");
        b.based_on = Some("A".into());
        sheet.push(a);
        sheet.push(b);
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new().style("A");
        let _ = cascade.paragraph(&para);
        assert!(cascade
            .diagnostics
            .iter()
            .any(|d| d.code == "display.style_cycle"));
    }

    #[test]
    fn a_num_id_sets_a_list_reference() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let mut def = NumDef::default();
        def.levels.insert(
            0,
            NumLevel {
                format: "decimal".into(),
                text: "%1.".into(),
                start: 1,
                ..NumLevel::default()
            },
        );
        cascade.add_numbering(1, def);
        let mut para = Paragraph::new();
        para.props.num_id = Some(1);
        para.props.ilvl = Some(0);
        let computed = cascade.paragraph(&para);
        assert_eq!(computed.list.map(|l| l.num_id), Some(1));
    }

    #[test]
    fn a_highlight_run_sets_a_shading_colour() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new();
        let mut props = CharProps::default();
        props.highlight = Some("yellow".into());
        let run = cascade.run(&para, &props);
        assert_eq!(run.highlight, Some(Rgba::opaque(0xFF, 0xFF, 0x00)));
    }

    #[test]
    fn a_bold_italic_run_carries_both_flags() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new();
        let mut props = CharProps::default();
        props.bold = true;
        props.italic = true;
        props.size_pt = Some(14.0);
        let run = cascade.run(&para, &props);
        assert!(run.bold && run.italic);
        assert!((run.size - 14.0).abs() < 0.01);
    }

    #[test]
    fn a_superscript_sets_the_vertical_align() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new();
        let mut props = CharProps::default();
        props.vert_align = Some("superscript".into());
        let run = cascade.run(&para, &props);
        assert_eq!(run.vert_align, Some(VertAlign::Superscript));
    }

    #[test]
    fn a_character_scale_produces_x_scale() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new();
        let mut props = CharProps::default();
        props.scale = Some(80);
        let run = cascade.run(&para, &props);
        assert!((run.x_scale - 0.8).abs() < 0.001);
    }

    #[test]
    fn a_run_with_no_size_uses_the_default() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new();
        let run = cascade.run(&para, &CharProps::default());
        assert!(run.size > 0.0);
    }

    #[test]
    fn a_negative_size_does_not_apply() {
        let sheet = StyleSheet::default();
        let mut cascade = Cascade::new(&sheet, &[]);
        let para = Paragraph::new();
        let mut props = CharProps::default();
        props.size_pt = Some(-4.0);
        let run = cascade.run(&para, &props);
        assert!(run.size > 0.0);
    }
}
