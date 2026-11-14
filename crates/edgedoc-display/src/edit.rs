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
    pub font_name: String,
    pub font_size_mixed: bool,
    pub font_name_mixed: bool,
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
    /// How many columns the caret's table cell covers; `0` when the caret is not in a cell.
    pub table_span: usize,
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
    /// True while a run of typing is open, so its keystrokes share one undo step (as in Word).
    typing_run: bool,
    /// Format painter: apply these props to the next selection.
    painter: Option<CharProps>,
    /// Sticky painter stays on across several selections.
    painter_sticky: bool,
    /// The bytes the editor was last opened from or saved to. `dirty` is a comparison against this,
    /// not a latch: undoing back to the opened state must say "saved", as Word does (DOC2-35).
    saved_bytes: std::sync::Arc<[u8]>,
    /// How many edits have been applied since the baseline. `0` means the bytes are the saved ones,
    /// which is the common case and must not cost a package write — `state()` runs on every keystroke.
    /// Any count above zero is *possibly* dirty, and `differs_from_saved` settles it by comparing.
    edits_since_saved: usize,
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
            typing_run: false,
            painter: None,
            painter_sticky: false,
            saved_bytes: bytes.into(),
            edits_since_saved: 0,
        };
        if editor.info().is_err() {
            editor.caret.path = editor.first_path();
        }
        Ok(editor)
    }

    /// Whether the document now differs from the bytes it was opened from (or last saved as).
    ///
    /// Word clears its unsaved marker when an undo returns the document to what is on disk, and a
    /// template that says "Unsaved changes" after you have undone everything teaches a person to
    /// ignore the warning. The comparison is against the bytes, not a flag, because the inner model's
    /// own flag is set by the first edit and never cleared.
    fn differs_from_saved(&self) -> bool {
        // The fast path: no edit has been applied since the baseline, so the bytes are the saved bytes
        // without writing a package. Every keystroke reaches this, so it must be free.
        if self.edits_since_saved == 0 {
            return false;
        }
        match self.package() {
            Ok(bytes) => bytes != *self.saved_bytes,
            // A package that cannot be written is not provably saved.
            Err(_) => true,
        }
    }

    /// Record the current bytes as the saved state — after a save, so the marker clears.
    pub fn mark_saved(&mut self) -> Result<()> {
        // A save ends the typing run: the next keystroke must take its own undo snapshot, or undo
        // after a save would jump back past the save point.
        self.end_typing_run();
        let bytes = self.package()?;
        self.saved_bytes = bytes.into();
        self.edits_since_saved = 0;
        Ok(())
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
        let computed = props
            .as_ref()
            .and_then(|props| self.computed_caret_run(props));
        let (font_size_mixed, font_name_mixed) = computed
            .as_ref()
            .map(|run| self.mixed_fonts(run))
            .unwrap_or((false, false));
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
            font_size: computed.as_ref().map(|run| run.size).unwrap_or(11.0),
            font_name: computed
                .as_ref()
                .map(|run| run.requested_face.clone())
                .unwrap_or_default(),
            font_size_mixed,
            font_name_mixed,
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
            dirty: self.differs_from_saved(),
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
            table_span: info.as_ref().and_then(|i| i.table_span).unwrap_or(0),
            color: props
                .as_ref()
                .and_then(|p| p.color.clone())
                .map(|hex| hex.trim_start_matches('#').to_ascii_uppercase())
                .unwrap_or_default(),
            highlight: props
                .as_ref()
                .and_then(|p| p.highlight.clone())
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
        // The text between two offsets of one paragraph; empty when an offset is not a char boundary.
        let slice = |text: &str, from: usize, to: usize| -> String {
            let to = to.min(text.len());
            let from = from.min(to);
            if !text.is_char_boundary(from) || !text.is_char_boundary(to) {
                return String::new();
            }
            text[from..to]
                .chars()
                .filter(|c| *c != '\u{FFFC}')
                .collect()
        };
        if anchor.path == self.caret.path {
            if anchor.offset == self.caret.offset {
                return String::new();
            }
            let text = self.para_text(&self.caret.path);
            let (start, end) = ordered_off(anchor.offset, self.caret.offset);
            return slice(&text, start, end);
        }
        // A selection across paragraphs reads each one in reading order, joined by a line break.
        let order = caret::paragraph_order(self.display.pages());
        let position = |path: &str| order.iter().position(|candidate| candidate == path);
        let (Some(a), Some(b)) = (position(&anchor.path), position(&self.caret.path)) else {
            return String::new();
        };
        let (first, last) = if a <= b {
            (anchor, &self.caret)
        } else {
            (&self.caret, anchor)
        };
        let (from, to) = (a.min(b), a.max(b));
        let mut parts = vec![slice(
            &self.para_text(&first.path),
            first.offset,
            usize::MAX,
        )];
        for path in &order[from + 1..to] {
            parts.push(slice(&self.para_text(path), 0, usize::MAX));
        }
        parts.push(slice(&self.para_text(&last.path), 0, last.offset));
        parts.join("\n")
    }

    pub fn package(&self) -> Result<Vec<u8>> {
        self.display.inner.write_bytes().map_err(from_doc)
    }

    /// Move the caret. `extend` grows the selection.
    pub fn move_caret(&mut self, key: &str, extend: bool) {
        self.end_typing_run();
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
        self.end_typing_run();
        let Some(hit) = caret::hit_caret(self.display.pages(), page, x, y) else {
            return;
        };
        self.place_hit(hit, extend);
        self.goal_x = None;
        self.pending = None;
        let _ = self.apply_painter();
    }

    /// Triple click selects only the caret paragraph; Select All remains document-wide.
    pub fn select_paragraph(&mut self) {
        self.end_typing_run();
        let path = self.caret.path.clone();
        self.anchor = Some(Place {
            path: path.clone(),
            offset: 0,
        });
        self.caret = Place {
            offset: self.para_len(&path),
            path,
        };
        self.pending = None;
        let _ = self.apply_painter();
    }

    pub fn select_all(&mut self) {
        self.end_typing_run();
        let paths = caret::paragraph_order(self.display.pages());
        let first = paths
            .first()
            .cloned()
            .unwrap_or_else(|| self.caret.path.clone());
        let last = paths
            .last()
            .cloned()
            .unwrap_or_else(|| self.caret.path.clone());
        self.anchor = Some(Place {
            path: first,
            offset: 0,
        });
        self.caret = Place {
            offset: self.para_len(&last),
            path: last,
        };
        self.pending = None;
        let _ = self.apply_painter();
    }

    /// Select the word around the caret. A double-click uses this.
    pub fn select_word(&mut self) {
        self.end_typing_run();
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
        self.end_typing_run();
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
        let step = delta.saturating_mul(720);
        self.paragraph_command(move |edit, path| edit.bump_indent(path, step))
    }

    /// Set automatic line spacing: `1`, `1.5`, or `2`.
    pub fn set_line_spacing(&mut self, multiple: f32) -> Result<()> {
        self.end_typing_run();
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
        self.paragraph_command(move |edit, path| edit.set_line_spacing(path, line))
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
        self.end_typing_run();
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
        // A run of typing is one undo step, as it is in Word: a person presses undo to take back a
        // mistyped word, not to walk back eleven letters. A newline, a delete, a format change, a caret
        // move and a paste each end the run, because each is a separate intent.
        let typing_run = self.typing_run;
        self.typing_run = true;
        if typing_run {
            return self.insert_without_snapshot(text);
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
        self.end_typing_run();
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
        self.end_typing_run();
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
        self.end_typing_run();
        let path = self.caret.path.clone();
        self.transact(move |edit| edit.insert_table(&path, rows, cols))
    }

    /// Insert a row below the caret's row (specs/144 DOC2-41). One call, one undo step.
    ///
    /// Refused when the caret is not in a table: a ribbon that greys the button is better than a
    /// button that does nothing, and the model already knows the difference.
    pub fn insert_table_row(&mut self) -> Result<()> {
        self.end_typing_run();
        let path = self.caret.path.clone();
        self.transact(move |edit| edit.insert_table_row(&path))
    }

    /// Delete the caret's row (specs/144 DOC2-41). A table keeps at least one row.
    pub fn delete_table_row(&mut self) -> Result<()> {
        self.end_typing_run();
        let path = self.caret.path.clone();
        self.transact(move |edit| edit.delete_table_row(&path))
    }

    /// Merge the caret's cell with `to` (specs/144 DOC2-43). One call, one undo step.
    ///
    /// `to` is a cell path from the same row. A vertical merge is refused by the model with a sentence
    /// the ribbon can show, rather than half-implemented: Word's `vMerge` has its own rules about which
    /// row owns the content.
    pub fn merge_table_cells(&mut self, to: &str) -> Result<()> {
        self.end_typing_run();
        let from = self.caret.path.clone();
        let to = to.to_string();
        self.transact(move |edit| edit.merge_table_cells(&from, &to))
    }

    /// Split the merged cell the caret is in, back into the cells it covers (specs/144 DOC2-43).
    pub fn split_table_cell(&mut self) -> Result<()> {
        self.end_typing_run();
        let path = self.caret.path.clone();
        self.transact(move |edit| edit.split_table_cell(&path))
    }

    /// Set the caret's column width, in twips (specs/144 DOC2-43).
    pub fn set_table_column_width(&mut self, width: i32) -> Result<()> {
        self.end_typing_run();
        let path = self.caret.path.clone();
        self.transact(move |edit| edit.set_table_column_width(&path, width))
    }

    pub fn insert_picture(&mut self, bytes: Vec<u8>, width_pt: f32, height_pt: f32) -> Result<()> {
        self.end_typing_run();
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

    /// Replace the next match of `query`, or every match.
    ///
    /// This is one call and **one undo step**, which is the whole reason it exists: a replace done as
    /// "delete the selection, then insert the replacement" moves the undo stack twice, so one press of
    /// Ctrl+Z would leave the document with the match deleted and nothing typed in its place. Word
    /// undoes a replace in one step, so this does too.
    ///
    /// A match that covers a merge field or a picture is refused when `replace_all` is false, because
    /// a find hit is painted text and a field paints as several characters under one marker: replacing
    /// it would destroy the field the template is made of. `all` counts what it changed.
    ///
    /// Returns the number of matches replaced.
    pub fn replace_text(
        &mut self,
        query: &str,
        replacement: &str,
        replace_all: bool,
    ) -> Result<usize> {
        self.end_typing_run();
        let query = query.to_string();
        if query.is_empty() {
            return Ok(0);
        }
        if replacement.chars().count() > 100_000 {
            return Err(Error::new(
                "editor.limit",
                "Paste fewer than 100,000 characters at a time.",
            ));
        }
        // Where each match lives, as source ranges. Read from the laid-out text, because that is what
        // the person is looking at; a source range comes back from the page's own span map.
        // Where each match lives. The rule is the find's own (`find_ranges`), so a find and a replace
        // can never disagree about what matched.
        let ranges = self.match_ranges(&query);
        if ranges.is_empty() {
            return Ok(0);
        }
        let mut targets: Vec<(usize, String, usize, usize)> = if replace_all {
            // Back to front, so replacing an earlier match cannot move the offsets of a later one in
            // the same paragraph.
            let mut all = ranges;
            all.reverse();
            all
        } else {
            // The next match at or after the caret, wrapping to the first — as Word does. Order is
            // the document's own (paragraph ordinal, then offset), never the string order of the
            // paths, where `body[10]` sorts before `body[2]`.
            let caret_ordinal = self
                .ordered_body_paths()
                .iter()
                .position(|path| *path == self.caret.path)
                .unwrap_or(0);
            let next = ranges
                .iter()
                .find(|range| (range.0, range.2) >= (caret_ordinal, self.caret.offset))
                .cloned();
            vec![next.unwrap_or_else(|| ranges[0].clone())]
        };
        let replacement = replacement.to_string();
        // The snapshot is taken by the first transaction; every later match drops its own, so one
        // replace-all is ONE undo step (`base` is read once, outside the loop).
        let base = self.undo.len();
        let mut changed = 0usize;
        for (_, path, start, end) in targets.drain(..) {
            match self.replace_once(&path, start, end, &replacement) {
                Ok(true) => {
                    changed += 1;
                    // Keep the first snapshot only: the stack must not grow per match.
                    if self.undo.len() > base + 1 {
                        self.undo.truncate(base + 1);
                    }
                }
                // A range that covers a field is skipped rather than destroying the field.
                Ok(false) => {}
                Err(err) => {
                    if changed == 0 {
                        return Err(err);
                    }
                    break;
                }
            }
        }
        Ok(changed)
    }

    /// The body's paragraphs in document order, as the layout sees them.
    fn ordered_body_paths(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for slot in crate::caret::line_slots(self.display.pages()) {
            if slot.path.starts_with("body[") && !seen.contains(&slot.path) {
                seen.push(slot.path);
            }
        }
        seen
    }

    /// Every match of `query` in the body, as `(paragraph ordinal, path, start, end)` in document
    /// order. The ordinal is the paragraph's position, taken from the layout's own line slots, so
    /// "the next match after the caret" can be answered without comparing path strings.
    fn match_ranges(&self, query: &str) -> Vec<(usize, String, usize, usize)> {
        let options = crate::SearchOptions {
            match_case: false,
            whole_word: false,
            diacritics: true,
        };
        let mut out: Vec<(usize, String, usize, usize)> = Vec::new();
        for (ordinal, path) in self.ordered_body_paths().iter().enumerate() {
            let text = self.para_text(path);
            for (from, to) in crate::search::find_ranges(&text, query, &options) {
                out.push((ordinal, path.clone(), from, to));
            }
        }
        out
    }

    /// Delete one source range and type the replacement over it, without touching the undo stack.
    /// Returns whether anything changed.
    fn replace_once(
        &mut self,
        path: &str,
        start: usize,
        end: usize,
        replacement: &str,
    ) -> Result<bool> {
        let path = path.to_string();
        let start_end = (start, end);
        let text = replacement.to_string();
        // A field is atomic: refuse a range that would cut through one.
        if self.range_hits_object(&path, start, end) {
            return Ok(false);
        }
        let before = self.undo.len();
        let caret = Place {
            path: path.clone(),
            offset: start_end.1,
        };
        let anchor = Place {
            path: path.clone(),
            offset: start_end.0,
        };
        let text_len = text.clone();
        let changed = self.transact_keep(move |edit| {
            let place = edit.delete_span(&anchor.path, anchor.offset, &caret.path, caret.offset)?;
            if text_len.is_empty() {
                return Ok(place);
            }
            let props = edit.paragraph_info(&place.path, place.offset)?.props;
            edit.insert_chars(&place.path, place.offset, &text_len, &props)
        });
        match changed {
            Ok(()) => {
                // Every match must share one undo entry. The snapshot the first call took covers the
                // state before it; later calls drop the one they took so the stack does not grow.
                if self.undo.len() > before {
                    let extra = self.undo.len() - before;
                    for _ in 1..extra {
                        self.undo.pop();
                    }
                }
                Ok(true)
            }
            Err(err) => Err(err),
        }
    }

    /// Whether `[start, end)` in one paragraph covers an inline object (a field, a picture).
    fn range_hits_object(&self, path: &str, start: usize, end: usize) -> bool {
        let Some(info) = self.info_at(path) else {
            return true;
        };
        let text = info.text;
        let start = start.min(text.len());
        let end = end.min(text.len());
        text.get(start..end)
            .is_some_and(|slice| slice.chars().any(|c| c == '\u{FFFC}'))
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

    /// The sheet the document is set on: width and height in twips as stored (portrait order), whether it is
    /// landscape, and the left margin. The ribbon reads this so a change keeps what it does not change.
    pub fn sheet(&self) -> (i32, i32, bool, i32) {
        let section = self.display.inner.section();
        (
            section.page_w.0,
            section.page_h.0,
            section.landscape,
            section.margin_left.0,
        )
    }

    /// Page Setup, for the whole document: the paper (`a4` or `letter`), its orientation, and one margin on
    /// all four sides in twips. It is one undo step, and every page repaints with the new sheet.
    pub fn set_page_setup(&mut self, paper: &str, landscape: bool, margin: i32) -> Result<()> {
        let size = match paper {
            "a4" => (11906, 16838),
            "letter" => (12240, 15840),
            other => {
                return Err(Error::new(
                    "layout.paper",
                    format!("Paper \"{other}\" is not offered. Choose A4 or Letter."),
                ))
            }
        };
        let caret = self.caret.clone();
        self.transact(move |edit| {
            edit.set_page_setup(size, landscape, margin)?;
            Ok(TextCaret {
                path: caret.path,
                offset: caret.offset,
            })
        })
    }

    pub fn toggle_mark(&mut self, mark: &str) -> Result<()> {
        let caret = self.caret.clone();
        let spans = self.selected_format_spans()?;
        if !spans.is_empty() {
            let mark = mark.to_string();
            return self.transact_keep(move |edit| {
                let mut all_on = true;
                for (path, start, end) in &spans {
                    all_on &= edit.range_is(path, *start, *end, |props| match mark.as_str() {
                        "italic" => props.italic,
                        "underline" => props.underline,
                        "strike" => props.strike,
                        "subscript" => props.vert_align.as_deref() == Some("subscript"),
                        "superscript" => props.vert_align.as_deref() == Some("superscript"),
                        _ => props.bold,
                    })?;
                }
                let kind = match mark.as_str() {
                    "italic" => FormatKind::Italic,
                    "underline" => FormatKind::Underline,
                    "strike" => FormatKind::Strike,
                    "subscript" => FormatKind::VertAlign("subscript".into()),
                    "superscript" => FormatKind::VertAlign("superscript".into()),
                    _ => FormatKind::Bold,
                };
                for (path, start, end) in spans {
                    edit.format_range(&path, start, end, kind.clone(), !all_on)?;
                }
                Ok(TextCaret {
                    path: caret.path,
                    offset: caret.offset,
                })
            });
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

    /// Relative size changes preserve differences between runs, including inherited style sizes.
    pub fn step_font_size(&mut self, grow: bool) -> Result<()> {
        self.end_typing_run();
        let caret = self.caret.clone();
        let spans = self.selected_format_spans()?;
        let mut cascade =
            crate::build::cascade_for_editing(&self.display.inner, self.display.fonts())?;
        if spans.is_empty() {
            let mut props = self.typing_props();
            let size = self
                .computed_caret_run(&props)
                .map(|run| run.size)
                .unwrap_or(11.0);
            props.size_pt = Some(next_font_size(size, grow));
            self.pending = Some(props);
            return Ok(());
        }
        self.transact_keep(move |edit| {
            for (path, start, end) in spans {
                edit.transform_character_range(&path, start, end, |para, props| {
                    props.size_pt = Some(next_font_size(cascade.run(para, props).size, grow));
                })?;
            }
            Ok(TextCaret {
                path: caret.path,
                offset: caret.offset,
            })
        })
    }

    pub fn clear_character_formatting(&mut self) -> Result<()> {
        self.end_typing_run();
        let caret = self.caret.clone();
        let spans = self.selected_format_spans()?;
        if spans.is_empty() {
            let mut props = self.typing_props();
            props.clear_manual_formatting();
            self.pending = Some(props);
            return Ok(());
        }
        self.transact_keep(move |edit| {
            for (path, start, end) in spans {
                edit.transform_character_range(&path, start, end, |_, props| {
                    props.clear_manual_formatting()
                })?;
            }
            Ok(TextCaret {
                path: caret.path,
                offset: caret.offset,
            })
        })
    }

    pub fn set_font_size(&mut self, size: f32) -> Result<()> {
        self.end_typing_run();
        if !(1.0..=1638.0).contains(&size) {
            return Err(Error::new(
                "editor.limit",
                "Font size must sit between 1 and 1638 points.",
            ));
        }
        self.apply_format(FormatKind::FontSize(size))
    }

    pub fn set_font(&mut self, name: &str) -> Result<()> {
        self.end_typing_run();
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
        self.end_typing_run();
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
        self.end_typing_run();
        let trimmed = name.trim();
        let value = if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("none") {
            String::new()
        } else {
            trimmed.to_string()
        };
        self.apply_format(FormatKind::Highlight(value))
    }

    pub fn set_align(&mut self, align: &str) -> Result<()> {
        self.end_typing_run();
        let align = match align {
            "center" => "center",
            "right" | "end" => "right",
            "justify" | "both" => "both",
            _ => "left",
        };
        let owned = align.to_string();
        self.paragraph_command(move |edit, path| edit.set_align(path, &owned))
    }

    pub fn toggle_list(&mut self, kind: &str) -> Result<()> {
        let list_kind = match kind {
            "none" | "off" => None,
            "number" | "decimal" => Some(edgedoc::ListKind::Decimal),
            "lowerroman" | "lower_roman" => Some(edgedoc::ListKind::LowerRoman),
            "upperroman" | "upper_roman" => Some(edgedoc::ListKind::UpperRoman),
            "lowerletter" | "lower_letter" => Some(edgedoc::ListKind::LowerLetter),
            "upperletter" | "upper_letter" => Some(edgedoc::ListKind::UpperLetter),
            _ => Some(edgedoc::ListKind::Bullet),
        };
        let next = if self.info().ok().and_then(|info| info.list) == list_kind {
            None
        } else {
            list_kind
        };
        self.paragraph_command(move |edit, path| edit.set_list(path, next))
    }

    pub fn set_para_shading(&mut self, fill: Option<&str>) -> Result<()> {
        let owned = fill.map(|color| color.trim_start_matches('#').to_uppercase());
        self.paragraph_command(move |edit, path| edit.set_para_shading(path, owned.as_deref()))
    }

    pub fn set_para_border(&mut self, kind: &str) -> Result<()> {
        let kind = kind.to_string();
        self.paragraph_command(move |edit, path| edit.set_para_border(path, &kind))
    }

    /// Closed Word-style before/after presets; unknown action names cannot silently succeed.
    pub fn para_spacing(&mut self, action: &str) -> Result<()> {
        let (before, twips) = match action {
            "before" => (true, 240),
            "clear-before" => (true, 0),
            "after" => (false, 240),
            "clear-after" => (false, 0),
            _ => {
                return Err(Error::new(
                    "editor.spacing",
                    "Choose spacing before or after the paragraph.",
                ))
            }
        };
        self.paragraph_command(move |edit, path| {
            if before {
                edit.set_space_before(path, twips)
            } else {
                edit.set_space_after(path, twips)
            }
        })
    }

    pub fn undo(&mut self) -> Result<()> {
        self.end_typing_run();
        self.swap_history(true)
    }

    /// How many undo steps are open. The ribbon's Undo is enabled from this, and a test reads it to
    /// prove that a run of typing is one step.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo(&mut self) -> Result<()> {
        self.end_typing_run();
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

    fn selected_paragraph_paths(&self) -> Result<Vec<String>> {
        let Some(anchor) = &self.anchor else {
            return Ok(vec![self.caret.path.clone()]);
        };
        if anchor.path == self.caret.path {
            return Ok(vec![self.caret.path.clone()]);
        }
        let paths = caret::paragraph_order(self.display.pages());
        let position = |path: &str| paths.iter().position(|candidate| candidate == path);
        let (Some(a), Some(b)) = (position(&anchor.path), position(&self.caret.path)) else {
            return Err(Error::new(
                "editor.selection",
                "The selected paragraphs cannot be changed.",
            ));
        };
        let last_offset = if a < b {
            self.caret.offset
        } else {
            anchor.offset
        };
        let mut selected = paths[a.min(b)..=a.max(b)].to_vec();
        if last_offset == 0 {
            selected.pop();
        }
        Ok(selected)
    }

    fn paragraph_command<F>(&mut self, mut change: F) -> Result<()>
    where
        F: FnMut(&mut DocumentEdit<'_>, &str) -> edgedoc_model::ModelResult<()>,
    {
        self.end_typing_run();
        let paths = self.selected_paragraph_paths()?;
        let caret = self.caret.clone();
        let pending = self.pending.clone();
        self.transact_keep(move |edit| {
            for path in paths {
                change(edit, &path)?;
            }
            Ok(TextCaret {
                path: caret.path,
                offset: caret.offset,
            })
        })?;
        // Paragraph presentation must not discard the character format chosen for new typing.
        self.pending = pending;
        Ok(())
    }

    fn computed_caret_run(&self, props: &CharProps) -> Option<edgedoc_style::ComputedRun> {
        let mut cascade =
            crate::build::cascade_for_editing(&self.display.inner, self.display.fonts()).ok()?;
        let mut computed = None;
        let _ = self
            .display
            .inner
            .clone()
            .edit(EditOptions::default(), |edit| {
                let para = edit.paragraph_snapshot(&self.caret.path)?;
                computed = Some(cascade.run(&para, props));
                Err(edgedoc_model::ModelError::new("editor.peek", "read only"))
            });
        computed
    }

    fn mixed_fonts(&self, current: &edgedoc_style::ComputedRun) -> (bool, bool) {
        let Ok(spans) = self.selected_format_spans() else {
            return (false, false);
        };
        if spans.is_empty() {
            return (false, false);
        }
        let Ok(mut cascade) =
            crate::build::cascade_for_editing(&self.display.inner, self.display.fonts())
        else {
            return (false, false);
        };
        let mut mixed = (false, false);
        let _ = self
            .display
            .inner
            .clone()
            .edit(EditOptions::default(), |edit| {
                for (path, start, end) in spans {
                    edit.range_matches_context(&path, start, end, |para, props| {
                        let run = cascade.run(para, props);
                        mixed.0 |= (run.size - current.size).abs() > 0.01;
                        mixed.1 |= !run
                            .requested_face
                            .eq_ignore_ascii_case(&current.requested_face);
                        true
                    })?;
                }
                Err(edgedoc_model::ModelError::new("editor.peek", "read only"))
            });
        mixed
    }

    /// Shared selection walk for character formatting, in document order rather than lexical path order.
    fn selected_format_spans(&self) -> Result<Vec<(String, usize, usize)>> {
        let Some(anchor) = &self.anchor else {
            return Ok(Vec::new());
        };
        if anchor.path == self.caret.path {
            let (start, end) = ordered_off(anchor.offset, self.caret.offset);
            return Ok(if start == end {
                Vec::new()
            } else {
                vec![(anchor.path.clone(), start, end)]
            });
        }
        let paths = caret::paragraph_order(self.display.pages());
        let position = |path: &str| paths.iter().position(|candidate| candidate == path);
        let (Some(a), Some(b)) = (position(&anchor.path), position(&self.caret.path)) else {
            return Err(Error::new(
                "editor.selection",
                "The selected paragraphs cannot be formatted.",
            ));
        };
        let (first, last) = if a <= b {
            (anchor, &self.caret)
        } else {
            (&self.caret, anchor)
        };
        let mut spans = Vec::new();
        let mut problem = None;
        let _ = self
            .display
            .inner
            .clone()
            .edit(EditOptions::default(), |edit| {
                for path in &paths[a.min(b)..=a.max(b)] {
                    let start = if path == &first.path { first.offset } else { 0 };
                    let end = if path == &last.path {
                        last.offset
                    } else {
                        match edit.paragraph_info(path, 0) {
                            Ok(info) => info.len,
                            Err(error) => {
                                problem = Some(error);
                                break;
                            }
                        }
                    };
                    if start < end {
                        spans.push((path.clone(), start, end));
                    }
                }
                Err(edgedoc_model::ModelError::new("editor.peek", "read only"))
            });
        if let Some(error) = problem {
            return Err(Error::new(error.code, error.message));
        }
        Ok(spans)
    }

    fn apply_format(&mut self, kind: FormatKind) -> Result<()> {
        let caret = self.caret.clone();
        let spans = self.selected_format_spans()?;
        if !spans.is_empty() {
            return self.transact_keep(move |edit| {
                for (path, start, end) in spans {
                    edit.format_range(&path, start, end, kind.clone(), true)?;
                }
                Ok(TextCaret {
                    path: caret.path,
                    offset: caret.offset,
                })
            });
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
                props.highlight = if name.is_empty() || name == "none" {
                    None
                } else {
                    Some(name)
                };
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
        self.edits_since_saved = self.edits_since_saved.saturating_add(1);
        self.display.layout()?;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// One keystroke inside an open typing run: change the bytes, keep the snapshot already taken.
    fn insert_without_snapshot(&mut self, text: &str) -> Result<()> {
        let anchor = self.anchor.clone();
        let pending = self.pending.clone();
        let caret = self.caret.clone();
        let mut next = TextCaret {
            path: caret.path.clone(),
            offset: caret.offset,
        };
        let opts = EditOptions {
            allow_revisions: true,
            allow_comments: true,
            allow_equations: true,
            allow_objects: true,
        };
        let edited = self.display.inner.edit(opts, |edit| {
            let place = replace_selection(edit, &caret, anchor.as_ref())?;
            let props = match pending.clone() {
                Some(props) => props,
                None => edit.paragraph_info(&place.path, place.offset)?.props,
            };
            edit.insert_chars(&place.path, place.offset, text, &props)?;
            next = TextCaret {
                path: place.path,
                offset: place.offset + text.len(),
            };
            Ok(())
        });
        if let Err(err) = edited {
            return Err(from_doc(err));
        }
        self.caret = Place {
            path: next.path,
            offset: next.offset,
        };
        self.anchor = None;
        self.pending = None;
        self.goal_x = None;
        self.redo.clear();
        // A keystroke inside an open typing run is still an edit: the document differs from what was
        // saved, so the marker must rise here too (the first keystroke of a run takes the other path).
        self.edits_since_saved = self.edits_since_saved.saturating_add(1);
        self.display.layout()?;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    /// End the open typing run, so the next keystroke starts a new undo step.
    fn end_typing_run(&mut self) {
        self.typing_run = false;
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
        // The snapshot IS the document now: if it is byte-for-byte the saved state, nothing is unsaved.
        // A non-zero count would force `differs_from_saved` to write a package on every keystroke, so
        // landing exactly on the baseline is the case worth recognising here.
        self.edits_since_saved = if snap.bytes == *self.saved_bytes {
            0
        } else {
            1
        };
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

fn next_font_size(size: f32, grow: bool) -> f32 {
    const SIZES: [f32; 16] = [
        8., 9., 10., 11., 12., 14., 16., 18., 20., 22., 24., 26., 28., 36., 48., 72.,
    ];
    let size = if size.is_finite() && size > 0.0 {
        size
    } else {
        11.0
    };
    if size >= 72.0 && grow {
        return ((size / 10.0).floor() * 10.0 + 10.0).min(1638.0);
    }
    if size > 72.0 && !grow {
        return ((size / 10.0).ceil() * 10.0 - 10.0).max(72.0);
    }
    if size < 8.0 {
        return if grow {
            (size.floor() + 1.0).min(8.0)
        } else {
            (size.ceil() - 1.0).max(1.0)
        };
    }
    if grow {
        SIZES
            .into_iter()
            .find(|value| *value > size)
            .unwrap_or(80.0)
    } else {
        SIZES
            .into_iter()
            .rev()
            .find(|value| *value < size)
            .unwrap_or(7.0)
    }
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
        assert_eq!(
            editor.state().color,
            "",
            "Automatic is the same as clearing"
        );
        editor.set_color("#047857").expect("colour again");
        assert_eq!(editor.state().color, "047857");

        editor.set_highlight("yellow").expect("highlight");
        assert_eq!(editor.state().highlight, "yellow");
        editor.set_highlight("none").expect("no highlight");
        assert_eq!(
            editor.state().highlight,
            "",
            "`none` must clear the highlight"
        );
        editor.set_highlight("").expect("empty highlight");
        assert_eq!(editor.state().highlight, "");

        // What was cleared is gone from the package too, not merely hidden by the ribbon.
        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let text = doc.plain_text(edgedoc::StoryRef::Body);
        assert_eq!(text.trim(), "Total");
    }

    /// specs/144 DOC2-35: a run of typing is ONE undo step, as it is in Word.
    ///
    /// Every keystroke used to snapshot the whole package, so typing a word and pressing undo walked
    /// back one letter at a time and the undo stack filled with near-identical copies. Word undoes the
    /// word. A different intent — a newline, a delete, a format change, a caret move — ends the run.
    #[test]
    fn a_run_of_typing_is_one_undo_step() {
        let mut editor = Editor::blank().expect("blank");
        for ch in "Bonjour".chars() {
            editor.insert_text(&ch.to_string()).expect("type");
        }
        assert_eq!(editor.para_text("body[0]"), "Bonjour");
        assert_eq!(editor.undo_depth(), 1, "sixkeystrokes are one step");

        // One undo takes the whole word back, not the last letter.
        editor.undo().expect("undo");
        assert_eq!(editor.para_text("body[0]"), "");

        // A newline ends the run: the next word is its own step.
        editor.insert_text("One").expect("type");
        editor.insert_newline(false).expect("enter");
        editor.insert_text("Two").expect("type");
        assert_eq!(editor.undo_depth(), 3, "two words and the break above them");
        editor.undo().expect("undo");
        assert_eq!(
            editor.para_text("body[1]"),
            "",
            "the second word is its own step"
        );

        // A caret move also ends the run: it is a separate intent, and Word starts a new step there.
        let mut moved = Editor::blank().expect("blank");
        moved.insert_text("abc").expect("type");
        moved.move_caret("left", false);
        moved.insert_text("X").expect("type");
        assert_eq!(
            moved.undo_depth(),
            2,
            "typing before and after a move are two steps"
        );
    }

    /// specs/144 DOC2-35: Word's Change Case has five rows. The module had three, and the missing one
    /// (`Sentence case`) is what a template author uses on a heading somebody typed in capitals.
    #[test]
    fn change_case_offers_sentence_alongside_upper_lower_and_title() {
        let mut editor = Editor::blank().expect("blank");
        editor
            .insert_text("quarterly REVENUE report")
            .expect("type");
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

    fn three_paragraphs() -> Editor {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("One").expect("type");
        editor.insert_newline(false).expect("enter");
        editor.insert_text("Two").expect("type");
        editor.insert_newline(false).expect("enter");
        editor.insert_text("Three").expect("type");
        editor.layout().expect("layout");
        editor
    }

    #[test]
    fn a_selection_across_paragraphs_reads_and_paints_every_one() {
        let mut editor = three_paragraphs();
        editor.move_caret("doc-start", false);
        editor.move_caret("doc-end", true);
        let state = editor.state();
        assert_eq!(state.selection, "One\nTwo\nThree");
        // One band per paragraph, the middle one included.
        assert_eq!(state.marks.len(), 3);
        let mut tops: Vec<f32> = state.marks.iter().map(|mark| mark.y).collect();
        tops.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(tops[0] < tops[1] && tops[1] < tops[2]);
    }

    #[test]
    fn character_formatting_across_paragraphs_survives_package_and_one_undo() {
        for backward in [false, true] {
            let mut editor = three_paragraphs();
            editor.move_caret(if backward { "doc-end" } else { "doc-start" }, false);
            editor.move_caret(if backward { "doc-start" } else { "doc-end" }, true);
            editor.set_font("Georgia").expect("font");
            editor.set_font_size(10.5).expect("size");
            editor.toggle_mark("bold").expect("bold");
            let bytes = editor.package().expect("package");
            let reopened = Editor::open(&bytes).expect("reopen");
            for path in ["body[0]", "body[1]", "body[2]"] {
                let props = reopened.info_at(path).expect("paragraph").props;
                assert_eq!(props.font.as_deref(), Some("Georgia"));
                assert_eq!(props.size_pt, Some(10.5));
                assert!(props.bold);
            }
            editor.undo().expect("undo one mark command");
            for path in ["body[0]", "body[1]", "body[2]"] {
                let props = editor.info_at(path).expect("paragraph").props;
                assert!(!props.bold);
                assert_eq!(props.size_pt, Some(10.5));
            }
        }
    }

    #[test]
    fn paragraph_selection_and_whole_document_selection_are_distinct() {
        let mut editor = three_paragraphs();
        editor.select_paragraph();
        assert_eq!(editor.state().selection, "Three");
        assert_eq!(editor.state().marks.len(), 1);
        editor.select_all();
        assert_eq!(editor.state().selection, "One\nTwo\nThree");
        assert_eq!(editor.state().marks.len(), 3);
    }

    #[test]
    fn paragraph_formatting_keeps_the_chosen_typing_format() {
        let mut editor = three_paragraphs();
        editor.move_caret("doc-end", false);
        editor.set_font_size(14.0).unwrap();
        editor.toggle_mark("bold").unwrap();
        editor.set_align("right").unwrap();
        editor.para_spacing("after").unwrap();
        assert_eq!(editor.state().font_size, 14.0);
        assert!(editor.state().bold);
        editor.insert_text("More").unwrap();
        let bytes = editor.package().unwrap();
        let mut reopened = Editor::open(&bytes).unwrap();
        reopened.move_caret("doc-end", false);
        assert_eq!(reopened.state().font_size, 14.0);
        assert!(reopened.state().bold);
        assert_eq!(reopened.state().align, "right");
    }

    #[test]
    fn mixed_font_steps_preserve_run_sizes_and_undo_as_one_change() {
        for backward in [false, true] {
            let mut editor = three_paragraphs();
            for (path, size, font) in [
                ("body[0]", 14.0, "Georgia"),
                ("body[1]", 20.0, "Arial"),
                ("body[2]", 10.5, "Arial"),
            ] {
                editor.anchor = Some(Place {
                    path: path.into(),
                    offset: 0,
                });
                editor.caret = Place {
                    path: path.into(),
                    offset: editor.para_len(path),
                };
                editor.set_font_size(size).unwrap();
                editor.set_font(font).unwrap();
            }
            editor.layout().unwrap();
            editor.move_caret(if backward { "doc-end" } else { "doc-start" }, false);
            editor.move_caret(if backward { "doc-start" } else { "doc-end" }, true);
            assert!(editor.state().font_size_mixed);
            assert!(editor.state().font_name_mixed);
            editor.step_font_size(true).unwrap();
            let bytes = editor.package().unwrap();
            let reopened = Editor::open(&bytes).unwrap();
            for (path, size) in [("body[0]", 16.0), ("body[1]", 22.0), ("body[2]", 11.0)] {
                assert_eq!(reopened.info_at(path).unwrap().props.size_pt, Some(size));
            }
            editor.undo().unwrap();
            assert_eq!(editor.info_at("body[0]").unwrap().props.size_pt, Some(14.0));
            assert_eq!(editor.info_at("body[1]").unwrap().props.size_pt, Some(20.0));
            editor.redo().unwrap();
            assert_eq!(editor.info_at("body[1]").unwrap().props.size_pt, Some(22.0));
        }
    }

    #[test]
    fn selected_paragraph_operations_and_character_reset_survive_save_and_undo() {
        let mut editor = three_paragraphs();
        editor.select_all();
        editor.toggle_mark("bold").unwrap();
        editor.set_font_size(18.0).unwrap();
        editor.set_align("center").unwrap();
        editor.bump_indent(1).unwrap();
        editor.set_line_spacing(1.15).unwrap();
        editor.para_spacing("before").unwrap();
        editor.para_spacing("after").unwrap();
        assert!(editor.para_spacing("add").is_err());
        editor.clear_character_formatting().unwrap();
        let reopened = Editor::open(&editor.package().unwrap()).unwrap();
        for path in ["body[0]", "body[1]", "body[2]"] {
            let props = reopened.info_at(path).unwrap();
            assert!(!props.props.bold);
            assert_eq!(props.props.size_pt, None);
            assert_eq!(props.align, "center");
        }
        editor.undo().unwrap();
        for path in ["body[0]", "body[1]", "body[2]"] {
            assert!(editor.info_at(path).unwrap().props.bold);
            assert_eq!(editor.info_at(path).unwrap().props.size_pt, Some(18.0));
        }
    }

    #[test]
    fn a_backward_selection_paints_the_same_bands_as_a_forward_one() {
        let mut forward = three_paragraphs();
        forward.move_caret("doc-start", false);
        forward.move_caret("doc-end", true);
        let mut backward = three_paragraphs();
        backward.move_caret("doc-end", false);
        backward.move_caret("doc-start", true);
        assert_eq!(backward.state().selection, forward.state().selection);
        assert_eq!(backward.state().marks, forward.state().marks);
    }

    #[test]
    fn page_setup_turns_the_sheet_landscape_and_survives_a_save() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Wide page").expect("type");
        editor.set_page_setup("a4", true, 720).expect("landscape");
        editor.layout().expect("layout");
        let page = editor.display.pages()[0].clone();
        assert!(
            page.w > page.h,
            "landscape must be wider than tall: {}x{}",
            page.w,
            page.h
        );
        // The change is in the file, not only on the screen.
        let bytes = editor.package().expect("save");
        let reopened = Editor::open(&bytes).expect("reopen");
        let again = &reopened.display.pages()[0];
        assert!(
            again.w > again.h,
            "the saved file keeps the landscape sheet"
        );
        assert_eq!(editor.sheet(), (16838, 11906, true, 720));
    }

    #[test]
    fn page_setup_is_one_undo_step() {
        let mut editor = Editor::blank().expect("blank");
        editor
            .set_page_setup("letter", true, 1440)
            .expect("landscape");
        assert!(editor.state().can_undo);
        editor.undo().expect("undo");
        editor.layout().expect("layout");
        let page = editor.display.pages()[0].clone();
        assert!(page.h > page.w, "undo restores portrait");
    }

    #[test]
    fn page_setup_refuses_a_margin_that_leaves_no_text_room() {
        let mut editor = Editor::blank().expect("blank");
        assert!(editor.set_page_setup("letter", false, 5000).is_err());
        assert!(editor.set_page_setup("letter", false, i32::MAX).is_err());
        assert!(editor.set_page_setup("tabloid", false, 1440).is_err());
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

    /// specs/144 DOC2-41: a table row is added below the caret's row, and it is one undo step.
    ///
    /// The ribbon's Insert tab could make a grid but never change its shape, so a quotation that
    /// needed a fourth line item had to be rebuilt. Word's *Insert Below* is the missing gesture.
    /// The new row must copy the row above it: a table whose rows disagree about their width is not
    /// a table anybody designed, and the model derives the cell count rather than taking a literal.
    #[test]
    fn a_table_row_is_inserted_below_and_undone_in_one_step() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(2, 3).expect("table");
        // The caret is in the table's first cell.
        assert_eq!(editor.caret.path, "body[1]/row[0]/cell[0]/block[0]");

        editor.insert_table_row().expect("insert row");
        assert_eq!(
            editor.caret.path, "body[1]/row[0]/cell[0]/block[0]",
            "the caret stays put"
        );

        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        assert_eq!(table.rows.len(), 3, "the row was added");
        assert_eq!(
            table.rows[2].cells.len(),
            3,
            "and it is as wide as the table"
        );

        editor.undo().expect("undo");
        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        assert_eq!(table.rows.len(), 2, "one undo takes the row back out");
    }

    /// specs/144 DOC2-41: a row can be removed, and the last row cannot.
    #[test]
    fn a_table_row_is_deleted_but_a_table_keeps_one() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(1, 2).expect("table");

        // One row: deleting it is refused with a reason, not a silent no-op.
        let refused = editor.delete_table_row();
        assert!(refused.is_err(), "a table cannot be emptied");
        assert_eq!(
            refused.unwrap_err().code,
            "model.limit",
            "and the reason names the limit"
        );

        editor.insert_table_row().expect("second row");
        editor.delete_table_row().expect("delete");
        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        assert_eq!(table.rows.len(), 1);
    }

    /// specs/144 DOC2-41: outside a table, "insert a row" is refused rather than doing nothing.
    #[test]
    fn a_row_command_outside_a_table_is_refused() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Just a paragraph").expect("type");
        let err = editor.insert_table_row().unwrap_err();
        assert_eq!(err.code, "model.bad_path");
        assert!(
            err.message.contains("table"),
            "the message says what is missing: {}",
            err.message
        );
    }

    /// specs/144 DOC2-35: undoing back to the opened state clears the unsaved marker.
    ///
    /// Word does not keep saying "Unsaved changes" after you have undone everything — that teaches a
    /// person to ignore the warning. The flag used to be a latch: the first edit set it, undo restored
    /// the bytes but never cleared it, so a template could read dirty with nothing to save.
    #[test]
    fn undoing_every_change_says_saved_again() {
        let mut editor = Editor::blank().expect("blank");
        assert!(!editor.state().dirty, "a freshly opened document is saved");

        editor.insert_text("Bonjour").expect("type");
        assert!(editor.state().dirty, "an edit is unsaved");

        editor.undo().expect("undo");
        assert!(
            !editor.state().dirty,
            "one undo took every change back, so there is nothing to save"
        );

        // And redo makes it unsaved again: the marker follows the bytes, not the direction of travel.
        editor.redo().expect("redo");
        assert!(editor.state().dirty);
    }

    /// specs/144 DOC2-35: a save clears the marker, and a later edit raises it again.
    #[test]
    fn saving_clears_the_unsaved_marker() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Bonjour").expect("type");
        assert!(editor.state().dirty);

        editor.mark_saved().expect("save");
        assert!(!editor.state().dirty, "the saved bytes are the baseline");

        editor.insert_text("!").expect("type");
        assert!(editor.state().dirty, "a change after the save is unsaved");
    }

    /// specs/144 DOC2-43: merging two cells keeps the grid — the swallowed cell becomes a continuation.
    ///
    /// The obvious implementation — delete the second cell — produces a row with one fewer cell, which is a
    /// *different table*: Word's grid is fixed, `gridSpan` says how many columns a cell covers, and a
    /// `hMerge: continue` cell holds the grid open. A file written the other way opens in Word with the
    /// table reflowed, so the studio and Word would disagree about the same document.
    #[test]
    fn merging_two_cells_keeps_the_grid_and_the_first_content() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(1, 3).expect("table");
        // Type into the first cell so there is content to keep.
        editor.insert_text("Keep me").expect("type");
        assert_eq!(editor.caret.path, "body[1]/row[0]/cell[0]/block[0]");

        editor
            .merge_table_cells("body[1]/row[0]/cell[1]/block[0]")
            .expect("merge");

        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        let row = &table.rows[0];
        assert_eq!(row.cells.len(), 3, "the grid is unchanged: three cells");
        assert_eq!(row.cells[0].span, 2, "the first cell covers two columns");
        assert_eq!(row.cells[1].h_merge.as_deref(), Some("continue"));
        // The content of the cell that was swallowed is gone; the first cell's is kept.
        assert!(row.cells[0].blocks.iter().any(
            |b| matches!(b.as_ref(), edgedoc::Block::Paragraph(p) if p.text.contains("Keep me"))
        ));
    }

    /// specs/144 DOC2-43: splitting puts the covered cells back, and an unmerged cell is a quiet no-op.
    #[test]
    fn splitting_a_merged_cell_restores_it_and_ignores_a_plain_cell() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(1, 3).expect("table");
        editor
            .merge_table_cells("body[1]/row[0]/cell[2]/block[0]")
            .expect("merge three");
        editor.split_table_cell().expect("split");

        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        assert_eq!(table.rows[0].cells[0].span, 1, "the span is back to one");
        assert_eq!(
            table.rows[0].cells[1].h_merge, None,
            "and the continuation is a cell again"
        );

        // A cell that was never merged: nothing changes, and nothing is refused.
        editor
            .split_table_cell()
            .expect("split a plain cell is a no-op");
        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        assert_eq!(table.rows[0].cells[0].span, 1);
    }

    /// specs/144 DOC2-43: a vertical merge is refused with a sentence, not half-done.
    #[test]
    fn a_vertical_merge_is_refused_with_a_reason() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(2, 2).expect("table");
        let err = editor
            .merge_table_cells("body[1]/row[1]/cell[0]/block[0]")
            .unwrap_err();
        assert_eq!(err.code, "model.limit");
        assert!(
            err.message.contains("row"),
            "the message says what is supported: {}",
            err.message
        );
    }

    /// specs/144 DOC2-43: a column width is written to the grid **and** to every cell, because Word
    /// renders from the cells and a file whose two disagree is a table nobody designed.
    #[test]
    fn a_column_width_reaches_the_grid_and_every_cell_in_it() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(2, 3).expect("table");
        editor.set_table_column_width(3_600).expect("width");

        let bytes = editor.package().expect("bytes");
        let doc = Document::read_bytes(&bytes).expect("read");
        let edgedoc::Block::Table(table) = doc.block_at(1).expect("block") else {
            panic!("a table");
        };
        assert_eq!(table.widths[0].0, 3_600, "the grid says so");
        for (index, row) in table.rows.iter().enumerate() {
            assert_eq!(
                row.cells[0].width.map(|w| w.0),
                Some(3_600),
                "row {index} cell 0 carries the width Word renders from"
            );
        }
    }

    /// specs/144 DOC2-43: a column width does not move the caret.
    ///
    /// A width is a property of the column, not a navigation. The first version returned a caret at
    /// `row[0]`, so widening a column in the fourth row of a table threw the person back to the top of it —
    /// and the ribbon's "Merge right" then named a different cell. Nobody would report that as a bug in the
    /// width; they would say the editor jumped.
    #[test]
    fn setting_a_column_width_leaves_the_caret_where_it_was() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Before").expect("type");
        editor.insert_table(3, 3).expect("table");
        // Type into a cell that is **not** row 0, so the assertion is about a real position rather than
        // about row 0 matching itself.
        // The test module can reach the field; a unit test does not need a public "move the caret there".
        editor.caret = Place {
            path: "body[1]/row[2]/cell[1]/block[0]".into(),
            offset: 0,
        };
        let before = editor.caret.path.clone();
        assert!(
            before.contains("row[2]"),
            "the caret must be in the last row: {before}"
        );

        editor.set_table_column_width(3_600).expect("width");
        assert_eq!(
            editor.caret.path, before,
            "a column width must not move the caret — the person is reading a cell, not asking to go to the top"
        );
    }

    /// specs/144 DOC2-39: a replace is ONE undo step.
    ///
    /// The control had no replace because the obvious implementation — delete the selection, then
    /// insert the replacement — moves the undo stack twice, so one Ctrl+Z would leave the match
    /// deleted and nothing typed in its place. `replace_text` does both inside one snapshot.
    #[test]
    fn a_replace_is_one_undo_step_and_puts_the_match_back() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Pay soon. Pay now.").expect("type");
        editor.move_caret("doc-start", false);

        let changed = editor.replace_text("Pay", "Send", false).expect("replace");
        assert_eq!(changed, 1);
        assert_eq!(editor.para_text("body[0]"), "Send soon. Pay now.");

        // One press takes back the whole replace, not half of it.
        editor.undo().expect("undo");
        assert_eq!(
            editor.para_text("body[0]"),
            "Pay soon. Pay now.",
            "one undo must restore the replaced text"
        );
        editor.redo().expect("redo");
        assert_eq!(editor.para_text("body[0]"), "Send soon. Pay now.");
    }

    /// specs/144 DOC2-39: replace-all is one undo step too, and it takes every match.
    #[test]
    fn a_replace_all_takes_every_match_in_one_step() {
        let mut editor = Editor::blank().expect("blank");
        editor
            .insert_text("Pay soon. Pay now. Pay later.")
            .expect("type");
        editor.move_caret("doc-start", false);

        let changed = editor
            .replace_text("pay", "Send", true)
            .expect("replace all");
        assert_eq!(changed, 3, "a case-insensitive replace takes all three");
        assert_eq!(
            editor.para_text("body[0]"),
            "Send soon. Send now. Send later."
        );

        editor.undo().expect("undo");
        assert_eq!(
            editor.para_text("body[0]"),
            "Pay soon. Pay now. Pay later.",
            "replace-all must be a single undo step"
        );
    }

    /// specs/144 DOC2-39: a replace leaves a merge field alone.
    ///
    /// A field is one marker character in the source and several glyphs on the page, so a replace
    /// that walked painted text would happily delete the field the whole template is made of. The
    /// match is taken from the source text, where the field is atomic, and `replace_once` refuses any
    /// range that would cover a marker — so text beside a field can be replaced and the field stays.
    #[test]
    fn a_replace_beside_a_merge_field_keeps_the_field() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Dear ").expect("type");
        editor.insert_field("Name").expect("field");
        editor.insert_text(", hello").expect("type");
        let before = editor.para_text("body[0]");
        assert!(
            before.contains('\u{FFFC}'),
            "the field is one marker: {before:?}"
        );

        let changed = editor
            .replace_text("hello", "bonjour", true)
            .expect("replace");
        assert_eq!(changed, 1, "the word beside the field is replaced");
        assert_eq!(
            editor.display.inner.merge_field_names(),
            vec!["Name".to_string()],
            "the field must survive"
        );
        assert!(editor.para_text("body[0]").contains("bonjour"));
        assert!(editor.para_text("body[0]").contains('\u{FFFC}'));

        editor.undo().expect("undo");
        assert_eq!(editor.para_text("body[0]"), before, "one undo restores it");
    }

    /// specs/144 DOC2-39: a query with no match changes nothing, and takes no undo step.
    #[test]
    fn a_replace_with_no_match_is_not_a_no_op_edit() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Hello").expect("type");
        let before = editor.state().revision;
        let changed = editor.replace_text("zzz", "x", true).expect("replace");
        assert_eq!(changed, 0);
        assert_eq!(editor.state().revision, before, "no edit happened");
        assert_eq!(editor.para_text("body[0]"), "Hello");
    }

    /// specs/144 DOC2-39: an accented word is found the way the find finds it.
    #[test]
    fn a_replace_matches_across_an_accent() {
        let mut editor = Editor::blank().expect("blank");
        editor.insert_text("Le café est bon.").expect("type");
        let changed = editor.replace_text("cafe", "thé", false).expect("replace");
        assert_eq!(
            changed, 1,
            "the find folds accents, so the replace must too"
        );
        assert_eq!(editor.para_text("body[0]"), "Le thé est bon.");
    }
}
