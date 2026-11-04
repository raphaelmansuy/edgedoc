use std::sync::atomic::{AtomicU64, Ordering};

use edgedoc_unit::{Emu, IntoTwips};

use crate::error::{ModelError, ModelResult};
use crate::fields::Field;
use crate::props::{CharProps, ListKind, ListSpec, ParaProps, Retention, TabStop};
use crate::table::Borders;

pub const OBJ: char = '\u{FFFC}';
pub const PAGE_BREAK: char = '\u{000C}';

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
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum InlineObject {
    Field(Field),
    Orphan,
    Picture(Picture),
    Opaque(String),
    Hyperlink {
        target: String,
    },
    Bookmark {
        name: String,
        start: bool,
    },
    /// A note or comment mark: the story it points to stays a part of the document.
    Note {
        kind: String,
        id: i32,
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
        if !self.text.is_char_boundary(offset) {
            return Err(ModelError::new(
                "model.bad_offset",
                format!("The offset {offset} is not a character boundary. Choose a boundary in the paragraph."),
            ));
        }
        let clean: String = text.chars().filter(|c| *c != OBJ).collect();
        self.text.insert_str(offset, &clean);
        self.retarget_runs(offset, clean.len() as isize, props);
        self.touch();
        Ok(())
    }

    fn retarget_runs(&mut self, offset: usize, delta: isize, props: &CharProps) {
        if delta <= 0 {
            return;
        }
        let add = delta as usize;
        let mut cursor = 0;
        for run in &mut self.runs {
            let end = cursor + run.len;
            if offset <= end {
                if run.props == *props && offset >= cursor {
                    run.len += add;
                    return;
                }
                break;
            }
            cursor = end;
        }
        self.runs.push(Run {
            len: add,
            props: props.clone(),
        });
    }

    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        let mut objects = self.objects.iter();
        for ch in self.text.chars() {
            if ch == OBJ {
                match objects.next() {
                    Some(InlineObject::Field(field)) => out.push_str(&field.result),
                    Some(InlineObject::Hyperlink { .. }) => {}
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

/// A run whose text is a tab character.
pub const TAB: &str = "\t";

/// A run whose text is a line break.
pub const LINE_BREAK: &str = "\n";
