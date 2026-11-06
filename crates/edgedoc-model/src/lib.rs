//! Document model. Blocks are `Arc` values, so a clone copies the pointers.
//! Mutation uses `&mut`. The model has no `RefCell`.

mod document;
mod error;
mod fields;
mod para;
mod props;
mod section;
mod style;
mod table;

pub use document::{BlockPath, BodyCursor, CoreProps, DocumentEdit, ModelDocument, ParaCursor};
pub use edgedoc_unit::Twips;
pub use error::{ModelError, ModelResult};
pub use fields::{placeholder, quote_name, Field, FormField, FormKind, Instruction};
pub use para::{
    Anchor, InlineObject, Paragraph, Picture, Run, RunBuilder, SectionBreak, COLUMN_BREAK,
    LINE_BREAK, OBJ, PAGE_BREAK, TAB,
};

pub use props::{
    CharProps, DropKind, EastAsianLayout, EditOptions, EditReport, FrameProps, ListKind, ListSpec,
    ParaProps, Retention, TabStop,
};
pub use section::{
    DocGrid, FootnotePr, HeaderRefs, LineNumbering, Part, PartKind, PartMeta, SectionProps,
    SectionStart, StoryRef,
};
pub use style::{heading_level, Style, StyleKind, StyleSheet, TableStyleRegion};
pub use table::{
    clamp_span, Block, Borders, Cell, CellMargins, ContentControl, ContentItem, LineBorder, Row,
    Table, TableLookOptions, TablePosition,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_edit_keeps_the_original_text() -> ModelResult<()> {
        let mut doc = ModelDocument::new();
        doc.body.push(std::sync::Arc::new(Block::Paragraph(
            Paragraph::new().text("Keep"),
        )));
        let err = doc.edit(EditOptions::default(), |edit| {
            edit.body().paragraph_after_last(|para| para.text("More"))?;
            Err(ModelError::new("model.bad_path", "stop"))
        });
        if err.is_ok() {
            return Err(ModelError::new("model.bad_path", "the edit should fail"));
        }
        if doc.plain_text(StoryRef::Body) != "Keep" {
            return Err(ModelError::new(
                "model.bad_path",
                doc.plain_text(StoryRef::Body),
            ));
        }
        Ok(())
    }

    #[test]
    fn offset_must_be_a_char_boundary() -> ModelResult<()> {
        let mut para = Paragraph::new().text("é");
        let err = para.insert_text(1, "x", &CharProps::default());
        match err {
            Err(err) if err.code == "model.bad_offset" => Ok(()),
            Ok(()) => Err(ModelError::new(
                "model.bad_offset",
                "the offset was accepted",
            )),
            Err(err) => Err(err),
        }
    }

    #[test]
    fn a_path_reaches_a_table_cell() -> ModelResult<()> {
        use std::sync::Arc;
        let mut doc = ModelDocument::new();
        let table = Table::new().row(["Cell"]);
        doc.body.push(Arc::new(Block::Table(table)));
        doc.edit(EditOptions::default(), |edit| {
            edit.body().paragraph_at_path(
                &BlockPath::Cell {
                    table: 0,
                    row: 0,
                    cell: 0,
                    block: 0,
                },
                |para| para.text(" More"),
            )
        })?;
        if doc.plain_text(StoryRef::Body) != "Cell More" {
            return Err(ModelError::new(
                "model.bad_path",
                doc.plain_text(StoryRef::Body),
            ));
        }
        Ok(())
    }
}
