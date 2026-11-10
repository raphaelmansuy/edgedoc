//! An editing session over a Word package.
//!
//! The session owns the document, the caret, and the undo stack. Every change goes through
//! [`edgedoc::Document::edit`], then a fresh layout. Layout of a business template is a few
//! milliseconds, so the caret paints from the same geometry as the page.

use edgedoc::{CoreProps, Document, Mm, Page, Paragraph};
use edgedoc_model::{CharProps, DocumentEdit, EditOptions, FormatKind, TextCaret};

use crate::caret::{self, CaretPoint, MarkBox};
use crate::error::{Error, Result};
use crate::{DisplayDocument, LayoutReport, OpenOptions};

/// One selection rectangle in page points.
pub type EditorMark = MarkBox;

/// The ribbon and the caret read this after every command.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorState {
    pub path: String,
    pub offset: usize,
    pub page: u32,
    pub x: f32,
    pub y: f32,
    pub h: f32,
    pub anchor_path: Option<String>,
    pub anchor_offset: Option<usize>,
    pub marks: Vec<EditorMark>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub subscript: bool,
    pub superscript: bool,
    pub font_size: f32,
    pub align: String,
    pub list: String,
    pub field: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub preview: bool,
    pub dirty: bool,
    pub words: usize,
    pub pages: u32,
    pub fields: Vec<String>,
    pub title: String,
    /// Bumps when the pages change. A caret move leaves it alone.
    pub revision: u64,
    /// The selected source text, with object markers removed.
    pub selection: String,
    /// Left indent in points.
    pub indent_left: f32,
    /// Automatic line spacing multiplier (`1`, `1.5`, `2`), or `0` when unset.
    pub line: f32,
    /// Named look guessed from the caret props.
    pub look: String,
    /// True while the format painter is armed.
    pub painter: bool,
    /// Paragraph background fill at the caret, without `#`.
    pub para_shading: String,
    pub para_border_bottom: bool,
    /// Text colour at the caret, without `#`; empty when the run inherits it (Word's Automatic).
    pub color: String,
    /// Highlight at the caret; empty when there is none.
    pub highlight: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Place {
    path: String,
    offset: usize,
}

#[derive(Clone)]
struct Snap {
    bytes: Vec<u8>,
    caret: Place,
    anchor: Option<Place>,
}

/// A document open for editing.
pub struct Editor {
    display: DisplayDocument,
    caret: Place,
    anchor: Option<Place>,
    pending: Option<CharProps>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    /// The template bytes while a merge preview is on screen.
    preview_of: Option<Vec<u8>>,
    goal_x: Option<f32>,
    revision: u64,
    /// Format painter: apply these props to the next selection.
    painter: Option<CharProps>,
    /// Sticky painter stays on across several selections.
    painter_sticky: bool,
}

impl Editor {
    /// A blank A4 page with one empty paragraph.
    pub fn blank() -> Result<Self> {
        let doc = Document::builder()
            .core(CoreProps::new().title("Untitled"))
            .page(Page::a4().margins(Mm(25.4)))
            .paragraph(Paragraph::new())
            .build()
            .map_err(from_doc)?;
        let bytes = doc.write_bytes().map_err(from_doc)?;
        Self::open(&bytes)
    }

    /// Open a package and place the caret in the first paragraph.
    pub fn open(bytes: &[u8]) -> Result<Self> {
        let mut display = DisplayDocument::open(bytes, &OpenOptions::default())?;
        display.layout()?;
        let mut editor = Self {
            display,
            caret: Place {
                path: "body[0]".into(),
                offset: 0,
            },
            anchor: None,
            pending: None,
            undo: Vec::new(),
            redo: Vec::new(),
            preview_of: None,
            goal_x: None,
            revision: 0,
            painter: None,
            painter_sticky: false,
        };
        if editor.info().is_err() {
            editor.caret.path = editor.first_path();
        }
        Ok(editor)
    }

    pub fn display(&self) -> &DisplayDocument {
        &self.display
    }

    pub fn layout(&mut self) -> Result<LayoutReport> {
        self.display.layout()
    }

    pub fn state(&self) -> EditorState {
        let point = caret::caret_point(self.display.pages(), &self.caret.path, self.caret.offset);
        let info = self.info().ok();
        let (anchor_path, anchor_offset, marks) = match &self.anchor {
            Some(anchor)
                if anchor.path != self.caret.path || anchor.offset != self.caret.offset =>
            {
                (
                    Some(anchor.path.clone()),
                    Some(anchor.offset),
                    caret::selection_marks(
                        self.display.pages(),
                        &anchor.path,
                        anchor.offset,
                        &self.caret.path,
                        self.caret.offset,
                    ),
                )
            }
            _ => (None, None, Vec::new()),
        };
        let props = self
            .pending
            .clone()
            .or_else(|| info.as_ref().map(|i| i.props.clone()));
        let words = self
            .display
            .inner
            .plain_text(edgedoc::StoryRef::Body)
            .split_whitespace()
            .count();
        EditorState {
            path: self.caret.path.clone(),
            offset: self.caret.offset,
            page: point.as_ref().map(|p| p.page).unwrap_or(0),
            x: point.as_ref().map(|p| p.x).unwrap_or(72.0),
            y: point.as_ref().map(|p| p.y).unwrap_or(72.0),
            h: point.as_ref().map(|p| p.h).unwrap_or(14.0),
            anchor_path,
            anchor_offset,
            marks,
            bold: props.as_ref().is_some_and(|p| p.bold),
            italic: props.as_ref().is_some_and(|p| p.italic),
            underline: props.as_ref().is_some_and(|p| p.underline),
            strike: props.as_ref().is_some_and(|p| p.strike),
            subscript: props
                .as_ref()
                .and_then(|p| p.vert_align.as_deref())
                .is_some_and(|v| v == "subscript"),
            superscript: props
                .as_ref()
                .and_then(|p| p.vert_align.as_deref())
                .is_some_and(|v| v == "superscript"),
            font_size: props.as_ref().and_then(|p| p.size_pt).unwrap_or(11.0),
            align: info
                .as_ref()
                .map(|i| i.align.clone())
                .unwrap_or_else(|| "left".into()),
            list: info
                .as_ref()
                .and_then(|i| i.list.map(|kind| format!("{kind:?}").to_ascii_lowercase()))
                .unwrap_or_default(),
            field: info.as_ref().and_then(|i| i.field.clone()),
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            preview: self.preview_of.is_some(),
            dirty: self.display.inner.is_dirty(),
            words,
            pages: self.display.pages().len() as u32,
            fields: self.display.inner.merge_field_names(),
            indent_left: info
                .as_ref()
                .map(|i| i.indent_left as f32 / 20.0)
                .unwrap_or(0.0),
            line: info
                .as_ref()
                .and_then(|i| i.line)
                .map(|twips| twips as f32 / 240.0)
                .unwrap_or(0.0),
            look: look_name(props.as_ref()),
            painter: self.painter.is_some(),
            para_shading: info
                .as_ref()
                .and_then(|i| i.shading.clone())
                .unwrap_or_default(),
            para_border_bottom: info.as_ref().is_some_and(|i| i.border_bottom),
            color: info
                .as_ref()
                .and_then(|i| i.props.color.clone())
                .map(|hex| hex.trim_start_matches('#').to_ascii_uppercase())
                .unwrap_or_default(),
            highlight: info
                .as_ref()
                .and_then(|i| i.props.highlight.clone())
                .filter(|name| !name.is_empty() && name != "none")
                .unwrap_or_default(),
            title: {
                let title = self.display.inner.core_props().title.clone();
                if title.is_empty() {
                    "Untitled".into()
                } else {
                    title
                }
            },
            revision: self.revision,
            selection: self.selection_text(),
        }
    }

    fn selection_text(&self) -> String {
        let Some(anchor) = &self.anchor else {
            return String::new();
        };
        if anchor.path != self.caret.path || anchor.offset == self.caret.offset {
            return String::new();
        }
        let text = self.para_text(&self.caret.path);
        let (start, end) = ordered_off(anchor.offset, self.caret.offset);
        let end = end.min(text.len());
        let start = start.min(end);
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return String::new();
        }
        text[start..end]
            .chars()
            .filter(|c| *c != '\u{FFFC}')
            .collect()
    }

    pub fn package(&self) -> Result<Vec<u8>> {
        self.display.inner.write_bytes().map_err(from_doc)
    }

    /// Move the caret. `extend` grows the selection.
    pub fn move_caret(&mut self, key: &str, extend: bool) {
        if extend && self.anchor.is_none() {
            self.anchor = Some(self.caret.clone());
        }
        if !extend {
            if let Some(anchor) = self.anchor.take() {
                if key == "left" || key == "right" {
                    let (path, offset) = if key == "left" {
                        min_place(&anchor, &self.caret)
                    } else {
                        max_place(&anchor, &self.caret)
                    };
                    self.caret = Place { path, offset };
                    self.pending = None;
                    return;
                }
            }
        }
        match key {
            "left" => self.step_char(-1),
            "right" => self.step_char(1),
            "up" => self.step_line(-1),
            "down" => self.step_line(1),
            "home" => self.line_edge(true),
            "end" => self.line_edge(false),
            "word-left" => self.step_word(-1),
            "word-right" => self.step_word(1),
            "doc-start" => self.doc_edge(true),
            "doc-end" => self.doc_edge(false),
            _ => {}
        }
        if key != "up" && key != "down" {
            self.goal_x = None;
        }
        self.pending = None;
    }

    pub fn click(&mut self, page: usize, x: f32, y: f32, extend: bool) {
        let Some(hit) = caret::hit_caret(self.display.pages(), page, x, y) else {
            return;
        };
        self.place_hit(hit, extend);
        self.goal_x = None;
        self.pending = None;
        let _ = self.apply_painter();
    }

    pub fn select_all(&mut self) {
        let path = self.caret.path.clone();
        let len = self.para_len(&path);
        self.anchor = Some(Place {
            path: path.clone(),
            offset: 0,
        });
        self.caret = Place { path, offset: len };
        self.pending = None;
        let _ = self.apply_painter();
    }

    /// Select the word around the caret. A double-click uses this.
    pub fn select_word(&mut self) {
        let text = self.para_text(&self.caret.path);
        let offset = self.caret.offset.min(text.len());
        if !text.is_char_boundary(offset) {
            return;
        }
        let start = if offset > 0 && text[..offset].chars().next_back().is_some_and(word_char) {
            word_left(&text, offset)
        } else {
            offset
        };
        let end = word_end(&text, start);
        if start == end {
            return;
        }
        let path = self.caret.path.clone();
        self.anchor = Some(Place {
            path: path.clone(),
            offset: start,
        });
        self.caret = Place { path, offset: end };
        self.pending = None;
        let _ = self.apply_painter();
    }

    /// Select from the previous sentence break to the next `.`, `!`, or `?`.
    pub fn select_sentence(&mut self) {
        let text = self.para_text(&self.caret.path);
        let offset = self.caret.offset.min(text.len());
        if !text.is_char_boundary(offset) {
            return;
        }
        let probe = sentence_probe(&text, offset);
        let start = sentence_start(&text, probe);
        let end = sentence_end(&text, probe).max(start);
        if start == end {
            return;
        }
        let path = self.caret.path.clone();
        self.anchor = Some(Place {
            path: path.clone(),
            offset: start,
        });
        self.caret = Place { path, offset: end };
        self.pending = None;
        let _ = self.apply_painter();
    }

    /// Arm or disarm the format painter. A second call with sticky keeps it on.
    pub fn arm_painter(&mut self, sticky: bool) {
        if self.painter.is_some() && !sticky {
            self.painter = None;
            self.painter_sticky = false;
            return;
        }
        self.painter = Some(self.typing_props());
        self.painter_sticky = sticky;
    }

    fn apply_painter(&mut self) -> Result<()> {
        let Some(props) = self.painter.clone() else {
            return Ok(());
        };
        let Some(anchor) = self.anchor.clone() else {
            return Ok(());
        };
        if anchor.path != self.caret.path || anchor.offset == self.caret.offset {
            return Ok(());
        }
        let caret = self.caret.clone();
        self.transact_keep(move |edit| {
            let (start, end) = ordered_off(anchor.offset, caret.offset);
            edit.format_range(
                &caret.path,
                start,
                end,
                FormatKind::FontSize(props.size_pt.unwrap_or(11.0)),
                true,
            )?;
            if let Some(font) = props.font.clone() {
                edit.format_range(&caret.path, start, end, FormatKind::Font(font), true)?;
            }
            if let Some(color) = props.color.clone() {
                edit.format_range(&caret.path, start, end, FormatKind::Color(color), true)?;
            }
            if let Some(highlight) = props.highlight.clone() {
                edit.format_range(
                    &caret.path,
                    start,
                    end,
                    FormatKind::Highlight(highlight),
                    true,
                )?;
            }
            edit.format_range(&caret.path, start, end, FormatKind::Bold, props.bold)?;
            edit.format_range(&caret.path, start, end, FormatKind::Italic, props.italic)?;
            edit.format_range(
                &caret.path,
                start,
                end,
                FormatKind::Underline,
                props.underline,
            )?;
            edit.format_range(&caret.path, start, end, FormatKind::Strike, props.strike)?;
            Ok(TextCaret {
                path: caret.path,
                offset: caret.offset,
            })
        })?;
        if !self.painter_sticky {
            self.painter = None;
        }
        Ok(())
    }

    /// Bump the left indent by half an inch when `delta` is ±1.
    pub fn bump_indent(&mut self, delta: i32) -> Result<()> {
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        let step = delta.saturating_mul(720);
        self.transact_keep(move |edit| {
            edit.bump_indent(&path, step)?;
            Ok(TextCaret { path, offset })
        })
    }

    /// Set automatic line spacing: `1`, `1.5`, or `2`.
    pub fn set_line_spacing(&mut self, multiple: f32) -> Result<()> {
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        let line = if (multiple - 1.0).abs() < 0.01 {
            240
        } else if (multiple - 1.5).abs() < 0.01 {
            360
        } else if (multiple - 2.0).abs() < 0.01 {
            480
        } else if multiple <= 0.0 {
            0
        } else {
            (multiple * 240.0).round() as i32
        };
        self.transact_keep(move |edit| {
            edit.set_line_spacing(&path, line)?;
            Ok(TextCaret { path, offset })
        })
    }

    /// Apply a named look: Normal, No Spacing, Heading, Heading 2, Title, or Subtitle.
    pub fn apply_look(&mut self, name: &str) -> Result<()> {
        let look = match name {
            "title" => Look::new(28.0, "Carlito", "1F4E79", true, false),
            "heading" => Look::new(16.0, "Carlito", "1F4E79", true, false),
            "heading2" => Look::new(13.0, "Carlito", "1F4E79", true, false),
            "subtitle" => Look::new(12.0, "Carlito", "5B6472", false, true),
            _ => Look::new(11.0, "Carlito", "000000", false, false),
        };
        self.apply_look_value(look)?;
        if name == "nospacing" {
            let path = self.caret.path.clone();
            let offset = self.caret.offset;
            self.transact_keep(move |edit| {
                edit.set_line_spacing(&path, 240)?;
                edit.set_space_after(&path, 0)?;
                Ok(TextCaret { path, offset })
            })?;
        }
        Ok(())
    }

    /// Change the case of the selection.
    pub fn change_case(&mut self, mode: &str) -> Result<()> {
        let Some(anchor) = self.anchor.clone() else {
            return Ok(());
        };
        if anchor.path != self.caret.path || anchor.offset == self.caret.offset {
            return Ok(());
        }
        let caret = self.caret.clone();
        let text = self.para_text(&caret.path);
        let (start, end) = ordered_off(anchor.offset, caret.offset);
        let end = end.min(text.len());
        let start = start.min(end);
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Ok(());
        }
        let next = transform_case(&text[start..end], mode);
        let written = next.clone();
        let path = caret.path.clone();
        self.transact(move |edit| {
            edit.delete_span(&path, start, &path, end)?;
            let props = edit.paragraph_info(&path, start)?.props;
            edit.insert_chars(&path, start, &written, &props)?;
            Ok(TextCaret {
                path: path.clone(),
                offset: start + written.len(),
            })
        })?;
        self.anchor = Some(Place {
            path: self.caret.path.clone(),
            offset: start,
        });
        Ok(())
    }

    fn apply_look_value(&mut self, look: Look) -> Result<()> {
        let caret = self.caret.clone();
        let anchor = self.anchor.clone();
        if let Some(anchor) = anchor {
            if anchor.path == caret.path && anchor.offset != caret.offset {
                return self.transact_keep(move |edit| {
                    let (start, end) = ordered_off(anchor.offset, caret.offset);
                    edit.format_range(
                        &caret.path,
                        start,
                        end,
                        FormatKind::FontSize(look.size),
                        true,
                    )?;
                    edit.format_range(
                        &caret.path,
                        start,
                        end,
                        FormatKind::Font(look.font.clone()),
                        true,
                    )?;
                    edit.format_range(
                        &caret.path,
                        start,
                        end,
                        FormatKind::Color(look.color.clone()),
                        true,
                    )?;
                    edit.format_range(&caret.path, start, end, FormatKind::Bold, look.bold)?;
                    edit.format_range(&caret.path, start, end, FormatKind::Italic, look.italic)?;
                    edit.format_range(&caret.path, start, end, FormatKind::Underline, false)?;
                    edit.format_range(&caret.path, start, end, FormatKind::Strike, false)?;
                    edit.format_range(
                        &caret.path,
                        start,
                        end,
                        FormatKind::VertAlign(String::new()),
                        false,
                    )?;
                    edit.format_range(
                        &caret.path,
                        start,
                        end,
                        FormatKind::Highlight("none".into()),
                        true,
                    )?;
                    Ok(TextCaret {
                        path: caret.path,
                        offset: caret.offset,
                    })
                });
            }
        }
        self.pending = Some(CharProps {
            size_pt: Some(look.size),
            font: Some(look.font),
            color: Some(look.color),
            bold: look.bold,
            italic: look.italic,
            underline: false,
            strike: false,
            vert_align: None,
            ..Default::default()
        });
        Ok(())
    }

    pub fn insert_text(&mut self, text: &str) -> Result<()> {
        if text.chars().count() > 100_000 {
            return Err(Error::new(
                "editor.limit",
                "Paste fewer than 100,000 characters at a time.",
            ));
        }
        let anchor = self.anchor.clone();
        let pending = self.pending.clone();
        let caret = self.caret.clone();
        self.transact(move |edit| {
            let place = replace_selection(edit, &caret, anchor.as_ref())?;
            let props = match pending {
                Some(props) => props,
                None => edit.paragraph_info(&place.path, place.offset)?.props,
            };
            edit.insert_chars(&place.path, place.offset, text, &props)
        })
    }

    pub fn insert_newline(&mut self, soft: bool) -> Result<()> {
        if soft {
            return self.insert_text("\n");
        }
        let anchor = self.anchor.clone();
        let caret = self.caret.clone();
        self.transact(move |edit| {
            let place = replace_selection(edit, &caret, anchor.as_ref())?;
            edit.break_paragraph(&place.path, place.offset)
        })
    }

    pub fn insert_field(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 64 {
            return Err(Error::new(
                "editor.field",
                "A merge field needs a name of 1 to 64 characters.",
            ));
        }
        if name.chars().any(|c| c.is_control()) {
            return Err(Error::new(
                "editor.field",
                "A merge field name cannot contain a control character.",
            ));
        }
        let anchor = self.anchor.clone();
        let caret = self.caret.clone();
        let owned = name.to_string();
        self.transact(move |edit| {
            let place = replace_selection(edit, &caret, anchor.as_ref())?;
            let props = edit.paragraph_info(&place.path, place.offset)?.props;
            edit.insert_merge_field(&place.path, place.offset, &owned, &props)
        })
    }

    pub fn insert_table(&mut self, rows: usize, cols: usize) -> Result<()> {
        let path = self.caret.path.clone();
        self.transact(move |edit| edit.insert_table(&path, rows, cols))
    }

    pub fn insert_picture(&mut self, bytes: Vec<u8>, width_pt: f32, height_pt: f32) -> Result<()> {
        if bytes.len() > 8 * 1024 * 1024 {
            return Err(Error::new(
                "editor.limit",
                "Choose an image smaller than 8 MB.",
            ));
        }
        let png = bytes.starts_with(&[0x89, b'P', b'N', b'G']);
        let jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
        if !png && !jpeg {
            return Err(Error::new("editor.image", "Insert a PNG or a JPEG image."));
        }
        let caret = self.caret.clone();
        let anchor = self.anchor.clone();
        self.transact(move |edit| {
            let place = replace_selection(edit, &caret, anchor.as_ref())?;
            let props = edit.paragraph_info(&place.path, place.offset)?.props;
            let picture = if png {
                edgedoc::Picture::png(bytes)
            } else {
                edgedoc::Picture::jpeg(bytes)
            };
            let picture = picture.size(
                edgedoc::Emu((f64::from(width_pt) * 12_700.0).round() as i64),
                edgedoc::Emu((f64::from(height_pt) * 12_700.0).round() as i64),
            );
            edit.insert_picture(&place.path, place.offset, picture, &props)
        })
    }

    pub fn delete(&mut self, backward: bool) -> Result<()> {
        if self.anchor.is_some() {
            let anchor = self.anchor.clone();
            let caret = self.caret.clone();
            return self.transact(move |edit| {
                replace_selection(edit, &caret, anchor.as_ref()).map(|place| TextCaret {
                    path: place.path,
                    offset: place.offset,
                })
            });
        }
        let caret = self.caret.clone();
        self.transact(move |edit| {
            let info = edit.paragraph_info(&caret.path, caret.offset)?;
            if backward {
                if caret.offset == 0 {
                    return edit.join_previous(&caret.path);
                }
                let start = prev_boundary(&info.text, caret.offset);
                edit.delete_span(&caret.path, start, &caret.path, caret.offset)
            } else if caret.offset >= info.len {
                let Some(next) = shift_path(&caret.path, 1) else {
                    return Ok(TextCaret {
                        path: caret.path,
                        offset: caret.offset,
                    });
                };
                if edit.paragraph_info(&next, 0).is_err() {
                    return Ok(TextCaret {
                        path: caret.path,
                        offset: caret.offset,
                    });
                }
                edit.join_previous(&next)
            } else {
                let end = next_boundary(&info.text, caret.offset);
                edit.delete_span(&caret.path, caret.offset, &caret.path, end)
            }
        })
    }

    pub fn toggle_mark(&mut self, mark: &str) -> Result<()> {
        let caret = self.caret.clone();
        let anchor = self.anchor.clone();
        if let Some(anchor) = anchor.clone() {
            if anchor.path == caret.path && anchor.offset != caret.offset {
                let mark = mark.to_string();
                return self.transact_keep(move |edit| {
                    let (start, end) = ordered_off(anchor.offset, caret.offset);
                    let enable = !match mark.as_str() {
                        "italic" => edit.range_is(&caret.path, start, end, |p| p.italic)?,
                        "underline" => edit.range_is(&caret.path, start, end, |p| p.underline)?,
                        "strike" => edit.range_is(&caret.path, start, end, |p| p.strike)?,
                        "subscript" => edit.range_is(&caret.path, start, end, |p| {
                            p.vert_align.as_deref() == Some("subscript")
                        })?,
                        "superscript" => edit.range_is(&caret.path, start, end, |p| {
                            p.vert_align.as_deref() == Some("superscript")
                        })?,
                        _ => edit.range_is(&caret.path, start, end, |p| p.bold)?,
                    };
                    let kind = match mark.as_str() {
                        "italic" => FormatKind::Italic,
                        "underline" => FormatKind::Underline,
                        "strike" => FormatKind::Strike,
                        "subscript" => FormatKind::VertAlign("subscript".into()),
                        "superscript" => FormatKind::VertAlign("superscript".into()),
                        _ => FormatKind::Bold,
                    };
                    edit.format_range(&caret.path, start, end, kind, enable)?;
                    Ok(TextCaret {
                        path: caret.path,
                        offset: caret.offset,
                    })
                });
            }
        }
        let mut props = self.typing_props();
        match mark {
            "italic" => props.italic = !props.italic,
            "underline" => props.underline = !props.underline,
            "strike" => props.strike = !props.strike,
            "subscript" => {
                props.vert_align = if props.vert_align.as_deref() == Some("subscript") {
                    None
                } else {
                    Some("subscript".into())
                };
            }
            "superscript" => {
                props.vert_align = if props.vert_align.as_deref() == Some("superscript") {
                    None
                } else {
                    Some("superscript".into())
                };
            }
            _ => props.bold = !props.bold,
        }
        self.pending = Some(props);
        Ok(())
    }

    pub fn set_font_size(&mut self, size: f32) -> Result<()> {
        if !(1.0..=1638.0).contains(&size) {
            return Err(Error::new(
                "editor.limit",
                "Font size must sit between 1 and 1638 points.",
            ));
        }
        self.apply_format(FormatKind::FontSize(size))
    }

    pub fn set_font(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 {
            return Err(Error::new("editor.font", "Choose a font name."));
        }
        self.apply_format(FormatKind::Font(name.to_string()))
    }

    /// Set a text colour, or clear it.
    ///
    /// Word's "Automatic" is a real choice, so an empty string (or `auto`) clears the colour instead of
    /// being refused: a ribbon offers "No colour" beside the palette, and the person expects it to work.
    pub fn set_color(&mut self, hex: &str) -> Result<()> {
        let trimmed = hex.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
            return self.apply_format(FormatKind::Color(String::new()));
        }
        let hex = normalize_hex(trimmed)
            .ok_or_else(|| Error::new("editor.color", "Use a color like #1F4E79."))?;
        self.apply_format(FormatKind::Color(hex))
    }

    /// Set a highlight, or clear it. `none` and an empty name both clear, as Word's "No Color" does.
    pub fn set_highlight(&mut self, name: &str) -> Result<()> {
        let trimmed = name.trim();
        let value = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            String::new()
        } else {
            trimmed.to_string()
        };
        self.apply_format(FormatKind::Highlight(value))
    }

    pub fn set_align(&mut self, align: &str) -> Result<()> {
        let align = match align {
            "center" => "center",
            "right" | "end" => "right",
            "justify" | "both" => "both",
            _ => "left",
        };
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        let owned = align.to_string();
        self.transact_keep(move |edit| {
            edit.set_align(&path, &owned)?;
            Ok(TextCaret { path, offset })
        })
    }

    pub fn toggle_list(&mut self, kind: &str) -> Result<()> {
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        if matches!(kind, "none" | "off") {
            return self.transact_keep(move |edit| {
                edit.set_list(&path, None)?;
                Ok(TextCaret { path, offset })
            });
        }
        let list_kind = match kind {
            "number" | "decimal" => edgedoc::ListKind::Decimal,
            "lowerroman" | "lower_roman" => edgedoc::ListKind::LowerRoman,
            "upperroman" | "upper_roman" => edgedoc::ListKind::UpperRoman,
            "lowerletter" | "lower_letter" => edgedoc::ListKind::LowerLetter,
            "upperletter" | "upper_letter" => edgedoc::ListKind::UpperLetter,
            _ => edgedoc::ListKind::Bullet,
        };
        let current = self.info().ok().and_then(|info| info.list);
        let next = if current == Some(list_kind) {
            None
        } else {
            Some(list_kind)
        };
        self.transact_keep(move |edit| {
            edit.set_list(&path, next)?;
            Ok(TextCaret { path, offset })
        })
    }

    pub fn set_para_shading(&mut self, fill: Option<&str>) -> Result<()> {
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        let owned = fill.map(|color| color.trim_start_matches('#').to_uppercase());
        self.transact_keep(move |edit| {
            edit.set_para_shading(&path, owned.as_deref())?;
            Ok(TextCaret { path, offset })
        })
    }

    pub fn set_para_border(&mut self, kind: &str) -> Result<()> {
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        let kind = kind.to_string();
        self.transact_keep(move |edit| {
            edit.set_para_border(&path, &kind)?;
            Ok(TextCaret { path, offset })
        })
    }

    /// Paragraph spacing shortcuts aligned with Word’s line-and-paragraph spacing menu.
    pub fn para_spacing(&mut self, action: &str) -> Result<()> {
        let path = self.caret.path.clone();
        let offset = self.caret.offset;
        let twips = match action {
            "before" => 240,
            "clear-before" => 0,
            "after" => 240,
            "clear-after" => 0,
            _ => return Ok(()),
        };
        self.transact_keep(move |edit| match action {
            "before" | "clear-before" => {
                edit.set_space_before(&path, twips)?;
                Ok(TextCaret { path, offset })
            }
            _ => {
                edit.set_space_after(&path, twips)?;
                Ok(TextCaret { path, offset })
            }
        })
    }

    pub fn undo(&mut self) -> Result<()> {
        self.swap_history(true)
    }

    pub fn redo(&mut self) -> Result<()> {
        self.swap_history(false)
    }

    /// Show one record in the template. The template itself stays on the undo stack.
    pub fn preview(&mut self, values: &[(String, String)]) -> Result<()> {
        let template = match &self.preview_of {
            Some(bytes) => bytes.clone(),
            None => {
                let bytes = self.package()?;
                self.preview_of = Some(bytes.clone());
                bytes
            }
        };
        let doc = Document::read_bytes(&template).map_err(from_doc)?;
        let headers: Vec<String> = values.iter().map(|(key, _)| key.clone()).collect();
        let row: Vec<String> = values.iter().map(|(_, value)| value.clone()).collect();
        let records = edgedoc::Records::from_rows(headers, vec![row])
            .map_err(|err| Error::new(err.code, err.message))?;
        let merged = doc
            .preview_with(&records, 0, edgedoc::MissingField::Placeholder)
            .map_err(from_doc)?;
        let bytes = merged.write_bytes().map_err(from_doc)?;
        self.reload(&bytes)
    }

    pub fn end_preview(&mut self) -> Result<()> {
        let Some(bytes) = self.preview_of.take() else {
            return Ok(());
        };
        self.reload(&bytes)
    }

    fn apply_format(&mut self, kind: FormatKind) -> Result<()> {
        let caret = self.caret.clone();
        let anchor = self.anchor.clone();
        if let Some(anchor) = anchor {
            if anchor.path == caret.path && anchor.offset != caret.offset {
                return self.transact_keep(move |edit| {
                    edit.format_range(&caret.path, anchor.offset, caret.offset, kind, true)?;
                    Ok(TextCaret {
                        path: caret.path,
                        offset: caret.offset,
                    })
                });
            }
        }
        let mut props = self.typing_props();
        match kind {
            FormatKind::Bold => props.bold = true,
            FormatKind::Italic => props.italic = true,
            FormatKind::Underline => props.underline = true,
            FormatKind::Strike => props.strike = true,
            FormatKind::FontSize(size) => props.size_pt = Some(size),
            FormatKind::Font(name) => props.font = Some(name),
            FormatKind::Color(hex) => props.color = if hex.is_empty() { None } else { Some(hex) },
            FormatKind::Highlight(name) => {
                props.highlight = if name.is_empty() || name == "none" { None } else { Some(name) };
            }
            FormatKind::VertAlign(name) => {
                props.vert_align = if name.is_empty() { None } else { Some(name) };
            }
        }
        self.pending = Some(props);
        Ok(())
    }

    fn transact<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut DocumentEdit<'_>) -> edgedoc_model::ModelResult<TextCaret>,
    {
        self.transact_with(f, false)
    }

    /// A format change keeps the selection, the way a ribbon button does.
    fn transact_keep<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut DocumentEdit<'_>) -> edgedoc_model::ModelResult<TextCaret>,
    {
        self.transact_with(f, true)
    }

    fn transact_with<F>(&mut self, f: F, keep_selection: bool) -> Result<()>
    where
        F: FnOnce(&mut DocumentEdit<'_>) -> edgedoc_model::ModelResult<TextCaret>,
    {
        if self.preview_of.is_some() {
            return Err(Error::new(
                "editor.preview",
                "Leave preview before editing. Preview shows a record and does not change the template.",
            ));
        }
        self.push_undo();
        let opts = EditOptions {
            allow_revisions: true,
            allow_comments: true,
            allow_equations: true,
            allow_objects: true,
        };
        let mut next = TextCaret {
            path: self.caret.path.clone(),
            offset: self.caret.offset,
        };
        let edited = self.display.inner.edit(opts, |edit| {
            next = f(edit)?;
            Ok(())
        });
        if let Err(err) = edited {
            self.undo.pop();
            return Err(from_doc(err));
        }
        self.caret = Place {
            path: next.path,
            offset: next.offset,
        };
        if !keep_selection {
            self.anchor = None;
        }
        self.pending = None;
        self.goal_x = None;
        self.redo.clear();
        self.display.layout()?;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    fn push_undo(&mut self) {
        let Ok(bytes) = self.package() else {
            return;
        };
        self.undo.push(Snap {
            bytes,
            caret: self.caret.clone(),
            anchor: self.anchor.clone(),
        });
        if self.undo.len() > 80 {
            self.undo.remove(0);
        }
    }

    fn swap_history(&mut self, undo: bool) -> Result<()> {
        if self.preview_of.is_some() {
            return Err(Error::new(
                "editor.preview",
                "Leave preview before undoing.",
            ));
        }
        let Some(snap) = (if undo {
            self.undo.pop()
        } else {
            self.redo.pop()
        }) else {
            return Ok(());
        };
        if let Ok(bytes) = self.package() {
            let current = Snap {
                bytes,
                caret: self.caret.clone(),
                anchor: self.anchor.clone(),
            };
            if undo {
                self.redo.push(current);
            } else {
                self.undo.push(current);
            }
        }
        self.caret = snap.caret;
        self.anchor = snap.anchor;
        self.pending = None;
        self.reload(&snap.bytes)
    }

    fn reload(&mut self, bytes: &[u8]) -> Result<()> {
        let mut display = DisplayDocument::open(bytes, &OpenOptions::default())?;
        display.layout()?;
        self.display = display;
        if self.info().is_err() {
            self.caret = Place {
                path: self.first_path(),
                offset: 0,
            };
            self.anchor = None;
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    fn place_hit(&mut self, hit: CaretPoint, extend: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret.clone());
            }
            self.caret = Place {
                path: hit.path,
                offset: hit.offset,
            };
            return;
        }
        if let Some(end) = hit.select_end {
            self.anchor = Some(Place {
                path: hit.path.clone(),
                offset: hit.offset,
            });
            self.caret = Place {
                path: hit.path,
                offset: end,
            };
            return;
        }
        self.anchor = None;
        self.caret = Place {
            path: hit.path,
            offset: hit.offset,
        };
    }

    fn step_word(&mut self, dir: isize) {
        let text = self.para_text(&self.caret.path);
        let offset = self.caret.offset.min(text.len());
        if !text.is_char_boundary(offset) {
            return;
        }
        if dir < 0 && offset == 0 {
            self.step_char(-1);
            return;
        }
        if dir > 0 && offset >= text.len() {
            self.step_char(1);
            return;
        }
        self.caret.offset = if dir < 0 {
            word_left(&text, offset)
        } else {
            word_right(&text, offset)
        };
    }

    fn doc_edge(&mut self, start: bool) {
        let slots = caret::line_slots(self.display.pages());
        let slot = if start {
            slots.iter().find(|slot| slot.path.starts_with("body["))
        } else {
            slots
                .iter()
                .rev()
                .find(|slot| slot.path.starts_with("body["))
        };
        let Some(slot) = slot else {
            return;
        };
        let offset = caret::slot_edge(self.display.pages(), slot, start);
        self.caret = Place {
            path: slot.path.clone(),
            offset,
        };
    }

    fn step_char(&mut self, dir: isize) {
        let text = self.para_text(&self.caret.path);
        let len = text.len();
        if dir < 0 {
            if self.caret.offset == 0 {
                if let Some(prev) = shift_path(&self.caret.path, -1) {
                    if self.info_at(&prev).is_some() {
                        let len = self.para_len(&prev);
                        self.caret = Place {
                            path: prev,
                            offset: len,
                        };
                    }
                }
                return;
            }
            self.caret.offset = prev_boundary(&text, self.caret.offset.min(len));
        } else if self.caret.offset >= len {
            if let Some(next) = shift_path(&self.caret.path, 1) {
                if self.info_at(&next).is_some() {
                    self.caret = Place {
                        path: next,
                        offset: 0,
                    };
                }
            }
        } else {
            self.caret.offset = next_boundary(&text, self.caret.offset);
        }
    }

    fn step_line(&mut self, dir: isize) {
        let slots = caret::line_slots(self.display.pages());
        let point = caret::caret_point(self.display.pages(), &self.caret.path, self.caret.offset);
        let Some(point) = point else {
            return;
        };
        let goal = self.goal_x.unwrap_or(point.x);
        self.goal_x = Some(goal);
        let current = slots.iter().position(|slot| {
            slot.path == self.caret.path
                && slot.page == point.page
                && point.y >= slot.top - 0.5
                && point.y <= slot.top + slot.height + 0.5
        });
        let Some(index) = current else {
            return;
        };
        let next = if dir < 0 {
            index.checked_sub(1)
        } else {
            index.checked_add(1).filter(|n| *n < slots.len())
        };
        let Some(next) = next else {
            return;
        };
        let slot = &slots[next];
        let offset = caret::slot_offset(self.display.pages(), slot, goal);
        self.caret = Place {
            path: slot.path.clone(),
            offset,
        };
    }

    fn line_edge(&mut self, start: bool) {
        let slots = caret::line_slots(self.display.pages());
        let point = caret::caret_point(self.display.pages(), &self.caret.path, self.caret.offset);
        let Some(point) = point else {
            return;
        };
        let Some(slot) = slots.iter().find(|slot| {
            slot.path == point.path
                && slot.page == point.page
                && (point.y - slot.top).abs() < slot.height + 1.0
        }) else {
            return;
        };
        self.caret.offset = caret::slot_edge(self.display.pages(), slot, start);
    }

    fn typing_props(&self) -> CharProps {
        if let Some(props) = &self.pending {
            return props.clone();
        }
        self.info().map(|info| info.props).unwrap_or_default()
    }

    fn info(&self) -> Result<edgedoc_model::ParagraphInfo> {
        self.peek(&self.caret.path, self.caret.offset)
            .ok_or_else(|| Error::new("model.bad_path", "The caret is not in a paragraph."))
    }

    fn info_at(&self, path: &str) -> Option<edgedoc_model::ParagraphInfo> {
        self.peek(path, 0)
    }

    fn peek(&self, path: &str, offset: usize) -> Option<edgedoc_model::ParagraphInfo> {
        let mut info = None;
        let opts = EditOptions::default();
        let _ = self.display.inner.clone().edit(opts, |edit| {
            info = edit.paragraph_info(path, offset).ok();
            Err(edgedoc_model::ModelError::new("editor.peek", "read only"))
        });
        info
    }

    fn para_text(&self, path: &str) -> String {
        self.info_at(path).map(|info| info.text).unwrap_or_default()
    }

    fn para_len(&self, path: &str) -> usize {
        self.info_at(path).map(|info| info.len).unwrap_or(0)
    }

    fn first_path(&self) -> String {
        let mut path = "body[0]".to_string();
        caret::line_slots(self.display.pages())
            .into_iter()
            .find_map(|slot| {
                if slot.path.starts_with("body[") {
                    path.clone_from(&slot.path);
                    Some(())
                } else {
                    None
                }
            });
        path
    }
}

fn replace_selection(
    edit: &mut DocumentEdit<'_>,
    caret: &Place,
    anchor: Option<&Place>,
) -> edgedoc_model::ModelResult<TextCaret> {
    let Some(anchor) = anchor else {
        return Ok(TextCaret {
            path: caret.path.clone(),
            offset: caret.offset,
        });
    };
    if anchor.path == caret.path && anchor.offset == caret.offset {
        return Ok(TextCaret {
            path: caret.path.clone(),
            offset: caret.offset,
        });
    }
    edit.delete_span(&anchor.path, anchor.offset, &caret.path, caret.offset)
}

fn from_doc(err: edgedoc::Error) -> Error {
    Error::new(err.code, err.message)
}

fn prev_boundary(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    if offset == 0 {
        return 0;
    }
    let mut index = offset - 1;
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn next_boundary(text: &str, offset: usize) -> usize {
    if offset >= text.len() || !text.is_char_boundary(offset) {
        return text.len();
    }
    match text[offset..].chars().next() {
        Some(ch) => offset + ch.len_utf8(),
        None => text.len(),
    }
}

fn shift_path(path: &str, delta: isize) -> Option<String> {
    let start = path.rfind('[')?;
    let end = path[start + 1..].find(']')? + start + 1;
    let number: usize = path[start + 1..end].parse().ok()?;
    let next = if delta < 0 {
        number.checked_sub(1)?
    } else {
        number.checked_add(delta as usize)?
    };
    Some(format!("{}{next}{}", &path[..start + 1], &path[end..]))
}

fn look_name(props: Option<&CharProps>) -> String {
    let Some(props) = props else {
        return "normal".into();
    };
    let size = props.size_pt.unwrap_or(11.0);
    let color = props.color.as_deref().unwrap_or("000000");
    if props.bold && (size - 28.0).abs() < 0.5 && color.eq_ignore_ascii_case("1F4E79") {
        "title".into()
    } else if props.bold && (size - 16.0).abs() < 0.5 && color.eq_ignore_ascii_case("1F4E79") {
        "heading".into()
    } else if props.bold && (size - 13.0).abs() < 0.5 && color.eq_ignore_ascii_case("1F4E79") {
        "heading2".into()
    } else if props.italic && (size - 12.0).abs() < 0.5 && color.eq_ignore_ascii_case("5B6472") {
        "subtitle".into()
    } else {
        "normal".into()
    }
}

struct Look {
    size: f32,
    font: String,
    color: String,
    bold: bool,
    italic: bool,
}

impl Look {
    fn new(size: f32, font: &str, color: &str, bold: bool, italic: bool) -> Self {
        Self {
            size,
            font: font.to_string(),
            color: color.to_string(),
            bold,
            italic,
        }
    }
}

fn transform_case(text: &str, mode: &str) -> String {
    match mode {
        "upper" => text.to_uppercase(),
        "lower" => text.to_lowercase(),
        // Word's "Sentence case": the first alphabetic character of the passage, then lowercase.
        "sentence" => {
            let mut out = String::with_capacity(text.len());
            let mut capitalised = false;
            for ch in text.chars() {
                if !capitalised && ch.is_alphabetic() {
                    out.extend(ch.to_uppercase());
                    capitalised = true;
                } else {
                    out.extend(ch.to_lowercase());
                }
            }
            out
        }
        "title" => text
            .split_inclusive(char::is_whitespace)
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect(),
        _ => text.to_string(),
    }
}

fn word_end(text: &str, start: usize) -> usize {
    let Some(rest) = text.get(start..) else {
        return start;
    };
    let mut end = start;
    for (index, ch) in rest.char_indices() {
        if !word_char(ch) {
            return start + index;
        }
        end = start + index + ch.len_utf8();
    }
    end
}

fn word_char(c: char) -> bool {
    !c.is_whitespace() && c != '\u{FFFC}' && c != '\u{000C}'
}

fn word_right(text: &str, offset: usize) -> usize {
    let chars: Vec<(usize, char)> = text[offset..]
        .char_indices()
        .map(|(index, ch)| (offset + index, ch))
        .collect();
    if chars.is_empty() {
        return offset;
    }
    let mut index = 0usize;
    if word_char(chars[0].1) {
        while index < chars.len() && word_char(chars[index].1) {
            index += 1;
        }
    }
    while index < chars.len() && !word_char(chars[index].1) {
        index += 1;
    }
    chars.get(index).map(|(at, _)| *at).unwrap_or(text.len())
}

fn sentence_break(c: char) -> bool {
    matches!(c, '.' | '!' | '?')
}

fn sentence_probe(text: &str, offset: usize) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut pos = offset.min(text.len());
    if pos == text.len() {
        pos -= 1;
    }
    while pos > 0 {
        let ch = text[pos..].chars().next().unwrap_or(' ');
        if sentence_break(ch) {
            pos -= 1;
        } else {
            break;
        }
    }
    pos
}

fn sentence_start(text: &str, probe: usize) -> usize {
    let mut i = probe.min(text.len());
    while i > 0 {
        i -= 1;
        let Some(ch) = text[i..].chars().next() else {
            break;
        };
        if sentence_break(ch) {
            let mut next = i + ch.len_utf8();
            while next < text.len() {
                let Some(c) = text[next..].chars().next() else {
                    break;
                };
                if c.is_whitespace() {
                    next += c.len_utf8();
                } else {
                    break;
                }
            }
            return next;
        }
    }
    0
}

fn sentence_end(text: &str, offset: usize) -> usize {
    let from = offset.min(text.len());
    for (i, c) in text[from..].char_indices() {
        if sentence_break(c) {
            return from + i + c.len_utf8();
        }
    }
    text.len()
}

fn word_left(text: &str, offset: usize) -> usize {
    let chars: Vec<(usize, char)> = text[..offset].char_indices().collect();
    if chars.is_empty() {
        return 0;
    }
    let mut index = chars.len();
    while index > 0 && !word_char(chars[index - 1].1) {
        index -= 1;
    }
    while index > 0 && word_char(chars[index - 1].1) {
        index -= 1;
    }
    chars.get(index).map(|(at, _)| *at).unwrap_or(0)
}

fn ordered_off(a: usize, b: usize) -> (usize, usize) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

fn min_place(a: &Place, b: &Place) -> (String, usize) {
    if a.path == b.path {
        if a.offset <= b.offset {
            (a.path.clone(), a.offset)
        } else {
            (b.path.clone(), b.offset)
        }
    } else if leaf_index(&a.path) <= leaf_index(&b.path) {
        (a.path.clone(), a.offset)
    } else {
        (b.path.clone(), b.offset)
    }
}

fn max_place(a: &Place, b: &Place) -> (String, usize) {
    let (path, offset) = min_place(a, b);
    if path == a.path && offset == a.offset {
        (b.path.clone(), b.offset)
    } else {
        (a.path.clone(), a.offset)
    }
}

fn leaf_index(path: &str) -> usize {
    path.rfind('[')
        .and_then(|start| {
            path[start + 1..]
                .find(']')
                .map(|end| &path[start + 1..start + 1 + end])
        })
        .and_then(|text| text.parse().ok())
        .unwrap_or(0)
}

fn normalize_hex(hex: &str) -> Option<String> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_uppercase())
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn typing_inserts_and_undo_restores() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Hello").expect("type");
        let text = editor.para_text("body[0]");
        assert_eq!(text, "Hello");
        assert_eq!(editor.caret.offset, 5);
        editor.undo().expect("undo");
        assert_eq!(editor.para_text("body[0]"), "");
        editor.redo().expect("redo");
        assert_eq!(editor.para_text("body[0]"), "Hello");
    }

    /// specs/144 DOC2-35: "No colour" and "No highlight" are real Word choices, and a ribbon offers
    /// them beside the palette. An empty value clears the format instead of being refused, and the
    /// package records the absence — not a black run, and not a stale one.
    #[test]
    fn clearing_a_colour_and_a_highlight_is_a_real_choice() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Total").expect("type");
        editor.select_all();
        editor.set_color("#B91C1C").expect("colour");
        assert_eq!(editor.state().color, "B91C1C");

        // Clearing is accepted three ways: empty, `auto` (Word's Automatic), and `none`.
        editor.set_color("").expect("clear");
        assert_eq!(editor.state().color, "", "an empty colour must clear it");
        editor.set_color("auto").expect("automatic");
        assert_eq!(editor.state().color, "", "Automatic is the same as clearing");
        editor.set_color("#047857").expect("colour again");
        assert_eq!(editor.state().color, "047857");

        editor.set_highlight("yellow").expect("highlight");
        assert_eq!(editor.state().highlight, "yellow");
        editor.set_highlight("none").expect("no highlight");
        assert_eq!(editor.state().highlight, "", "`none` must clear the highlight");
        editor.set_highlight("").expect("empty highlight");
        assert_eq!(editor.state().highlight, "");

        // What was cleared is gone from the package too, not merely hidden by the ribbon.
        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let text = doc.plain_text(edgedoc::StoryRef::Body);
        assert_eq!(text.trim(), "Total");
    }

    /// specs/144 DOC2-35: Word's Change Case has five rows. The module had three, and the missing one
    /// (`Sentence case`) is what a template author uses on a heading somebody typed in capitals.
    #[test]
    fn change_case_offers_sentence_alongside_upper_lower_and_title() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("quarterly REVENUE report").expect("type");
        editor.select_all();

        editor.change_case("upper").expect("upper");
        assert_eq!(editor.para_text("body[0]"), "QUARTERLY REVENUE REPORT");

        editor.change_case("lower").expect("lower");
        assert_eq!(editor.para_text("body[0]"), "quarterly revenue report");

        editor.change_case("sentence").expect("sentence");
        assert_eq!(editor.para_text("body[0]"), "Quarterly revenue report");

        editor.change_case("title").expect("title");
        assert_eq!(editor.para_text("body[0]"), "Quarterly Revenue Report");

        // An unknown mode leaves the text alone rather than mangling it.
        editor.change_case("nonsense").expect("unknown is a no-op");
        assert_eq!(editor.para_text("body[0]"), "Quarterly Revenue Report");
    }

    #[test]
    fn a_merge_field_round_trips_into_the_package() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Dear ").expect("type");
        editor.insert_field("Name").expect("field");
        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let text = doc.plain_text(edgedoc::StoryRef::Body);
        assert_eq!(doc.merge_field_names(), vec!["Name".to_string()], "{text}");
        assert!(doc.plain_text(edgedoc::StoryRef::Body).contains("«Name»"));
    }

    #[test]
    fn enter_splits_the_paragraph() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("ab").expect("type");
        editor.move_caret("left", false);
        editor.insert_newline(false).expect("enter");
        assert_eq!(editor.para_text("body[0]"), "a");
        assert_eq!(editor.para_text("body[1]"), "b");
        assert_eq!(editor.caret.path, "body[1]");
    }

    #[test]
    fn bold_marks_the_selection() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Hello").expect("type");
        editor.select_all();
        editor.toggle_mark("bold").expect("bold");
        let bytes = editor.package().expect("bytes");
        let again = Document::read_bytes(&bytes).expect("read");
        let block = again.block_at(0).expect("block");
        let edgedoc::Block::Paragraph(para) = block else {
            panic!("paragraph");
        };
        assert!(para.props_at(1).bold);
    }

    #[test]
    fn superscript_marks_the_selection() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("m2").expect("type");
        editor.select_word();
        editor.toggle_mark("superscript").expect("super");
        assert!(editor.state().superscript);
        let bytes = editor.package().expect("bytes");
        let again = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Paragraph(para) = again.block_at(0).expect("block") else {
            panic!("paragraph");
        };
        assert_eq!(para.props_at(1).vert_align.as_deref(), Some("superscript"));
    }

    #[test]
    fn backspace_joins_paragraphs() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("a").expect("a");
        editor.insert_newline(false).expect("enter");
        editor.insert_text("b").expect("b");
        editor.move_caret("home", false);
        let state = editor.state();
        assert_eq!(
            (state.path.as_str(), state.offset),
            ("body[1]", 0),
            "home should sit at the start of the second paragraph"
        );
        editor.delete(true).expect("backspace");
        assert_eq!(editor.para_text("body[0]"), "ab");
    }

    #[test]
    fn limits_refuse_a_huge_paste_a_bad_picture_and_an_empty_field() {
        let mut editor = Editor::blank().expect("blank");
        let paste = editor.insert_text(&"a".repeat(100_001)).expect_err("paste");
        assert_eq!(paste.code, "editor.limit");
        let picture = editor
            .insert_picture(vec![1, 2, 3, 4], 40.0, 40.0)
            .expect_err("picture");
        assert_eq!(picture.code, "editor.image");
        let field = editor.insert_field("").expect_err("field");
        assert_eq!(field.code, "editor.field");
        assert_eq!(editor.para_text("body[0]"), "");
    }

    #[test]
    fn a_double_click_selects_the_word_and_a_look_restyles_it() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Hello world").expect("type");
        editor.move_caret("left", false);
        editor.select_word();
        assert_eq!(editor.state().selection, "world");
        editor.change_case("upper").expect("case");
        assert_eq!(editor.para_text("body[0]"), "Hello WORLD");
        editor.select_word();
        editor.apply_look("title").expect("look");
        assert_eq!(editor.state().selection, "WORLD");
        assert!((editor.state().font_size - 28.0).abs() < 0.1);
    }

    #[test]
    fn word_motion_lands_on_the_next_word() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Hello world").expect("type");
        editor.move_caret("doc-start", false);
        editor.move_caret("word-right", false);
        assert_eq!(editor.state().offset, 6);
        editor.move_caret("word-left", false);
        assert_eq!(editor.state().offset, 0);
    }

    #[test]
    fn select_sentence_grabs_one_stop() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Pay soon. Invoice now.").expect("type");
        editor.move_caret("doc-end", false);
        editor.select_sentence();
        assert_eq!(editor.state().selection, "Invoice now.");
        editor.move_caret("doc-start", false);
        editor.select_sentence();
        assert_eq!(editor.state().selection, "Pay soon.");
    }

    #[test]
    fn select_all_reports_the_paragraph_text() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Hello").expect("type");
        let before = editor.state().revision;
        editor.select_all();
        let state = editor.state();
        assert_eq!(state.selection, "Hello");
        assert_eq!(state.revision, before);
    }

    #[test]
    fn preview_fills_a_field_without_losing_the_template() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_field("City").expect("field");
        editor
            .preview(&[("City".into(), "Lyon".into())])
            .expect("preview");
        assert!(editor.state().preview);
        assert!(editor
            .display
            .inner
            .plain_text(edgedoc::StoryRef::Body)
            .contains("Lyon"));
        let refused = editor.insert_text("x");
        assert!(refused.is_err());
        editor.end_preview().expect("end");
        assert!(!editor.state().preview);
        assert_eq!(
            editor.display.inner.merge_field_names(),
            vec!["City".to_string()]
        );
    }
}
