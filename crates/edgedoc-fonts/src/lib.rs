//! Font database, shaping, and Word-compatible face substitution.
//!
//! Shaping runs through `rustybuzz`, and font parsing through `ttf-parser`. Neither crate knows
//! about Word. The substitution table turns a document face name into a face that is available,
//! and it keeps the metrics of the requested face where a metric-compatible substitute exists.
//! See `DP-F-01` to `DP-F-15`, `DP-F-45`, and `DP-F-46`.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![deny(clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod symbols;

use std::collections::BTreeMap;
use std::sync::Arc;

use edgedoc_geom::Rgba;
use ttf_parser::{Face, GlyphId};
use unicode_segmentation::UnicodeSegmentation;

// Macintosh Roman names are byte-compatible with UTF-8 when they are ASCII.
fn ascii_mac_name(platform: ttf_parser::PlatformId, encoding: u16, bytes: &[u8]) -> Option<String> {
    if platform == ttf_parser::PlatformId::Macintosh
        && encoding == 0
        && !bytes.is_empty()
        && bytes.is_ascii()
    {
        std::str::from_utf8(bytes).ok().map(str::to_string)
    } else {
        None
    }
}

/// Largest font file the library accepts, in bytes.
pub const MAX_FONT_BYTES: usize = 8 << 20;

/// A face that the database holds.
#[derive(Debug, Clone)]
pub struct FontFace {
    pub id: u32,
    /// The family name from the face, which is what a document asks for.
    pub family: String,
    pub bold: bool,
    pub italic: bool,
    /// The `unitsPerEm` value.
    pub units_per_em: f32,
    pub ascender: f32,
    pub descender: f32,
    pub line_gap: f32,
    /// True when the family had to be substituted, so a reader must be told.
    pub substituted: bool,
    /// The family that the document asked for.
    pub requested: String,
    bytes: Arc<Vec<u8>>,
    /// Advance of every glyph id, in font units.
    advances: Vec<u16>,
    /// Bounding box of every glyph id, in font units: `(x0, y0, x1, y1)`.
    bboxes: Vec<[f32; 4]>,
    glyph_count: u16,
}

impl PartialEq for FontFace {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl FontFace {
    /// The exact font bytes used for shaping and outlines.
    pub fn data(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    pub fn ascender_pt(&self, size: f32) -> f32 {
        self.ascender / self.units_per_em * size
    }

    pub fn descender_pt(&self, size: f32) -> f32 {
        self.descender / self.units_per_em * size
    }

    /// The natural line height at a size, which is `ascent + descent + line gap`.
    pub fn line_height(&self, size: f32) -> f32 {
        (self.ascender + self.descender + self.line_gap) / self.units_per_em * size
    }

    /// The advance of a glyph at a size, in points.
    pub fn advance(&self, gid: u16, size: f32) -> f32 {
        let a = self.advances.get(gid as usize).copied().unwrap_or(0);
        f32::from(a) / self.units_per_em * size
    }

    /// The `unitsPerEm`-normalized advance, which shaping uses before it scales.
    pub fn advance_units(&self, gid: u16) -> f32 {
        f32::from(self.advances.get(gid as usize).copied().unwrap_or(0))
    }

    pub fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    /// The glyph for a character, or glyph 0 when the face has none.
    pub fn glyph_for(&self, c: char) -> Option<u16> {
        let face = Face::parse(self.bytes.as_slice(), 0).ok()?;
        face.glyph_index(c).map(|g| g.0).filter(|g| *g != 0)
    }

    pub fn has_char(&self, c: char) -> bool {
        self.glyph_for(c).is_some()
    }

    /// The bounding box of a glyph at a size, in points, with `y` up from the baseline.
    pub fn bbox(&self, gid: u16, size: f32) -> [f32; 4] {
        let b = self.bboxes.get(gid as usize).copied().unwrap_or([0.0; 4]);
        let k = size / self.units_per_em;
        [b[0] * k, b[1] * k, b[2] * k, b[3] * k]
    }

    /// The outline of a glyph in font units, as a path of line and curve segments.
    pub fn outline(&self, gid: u16) -> Vec<PathSeg> {
        let Ok(face) = Face::parse(self.bytes.as_slice(), 0) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut builder = PathBuilder { segs: &mut out };
        let _ = face.outline_glyph(GlyphId(gid), &mut builder);
        out
    }

    /// The colour layers of a colour glyph, if a later version reads `COLR`.
    ///
    /// Version 1 paints the fallback glyph. A colour font is rare in a business document, and
    /// `DP-P-10` already covers an unsupported image. See [15-plan.md](../../docs/display/15-plan.md).
    pub fn color_layers(&self, _gid: u16) -> Option<Vec<(u16, Rgba)>> {
        None
    }
}

/// One segment of a glyph outline, in font units with `y` up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathSeg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

struct PathBuilder<'a> {
    segs: &'a mut Vec<PathSeg>,
}

impl ttf_parser::OutlineBuilder for PathBuilder<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.segs.push(PathSeg::Move(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.segs.push(PathSeg::Line(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.segs.push(PathSeg::Quad(x1, y1, x, y));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.segs.push(PathSeg::Cubic(x1, y1, x2, y2, x, y));
    }

    fn close(&mut self) {
        self.segs.push(PathSeg::Close);
    }
}

/// A shaped glyph: the id, and its offset from the pen in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    pub gid: u16,
    /// Shaped advance, including kerning and positioning adjustments.
    pub advance: f32,
    pub dx: f32,
    pub dy: f32,
    /// Byte offset of the cluster in the run text.
    pub cluster: u32,
}

/// A face database. Faces load lazily and stay for the session.
#[derive(Debug, Default)]
pub struct FontDb {
    faces: Vec<FontFace>,
    by_key: BTreeMap<(String, bool, bool), u32>,
    fallback: Option<u32>,
}

impl FontDb {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.faces.len()
    }

    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    pub fn faces(&self) -> &[FontFace] {
        &self.faces
    }

    /// Every registered family/style alias, including aliases of metric-compatible faces.
    pub fn registered_faces(&self) -> impl Iterator<Item = (&str, bool, bool, u32)> {
        self.by_key
            .iter()
            .map(|((family, bold, italic), id)| (family.as_str(), *bold, *italic, *id))
    }

    pub fn face(&self, id: u32) -> Option<&FontFace> {
        self.faces.get(id as usize)
    }

    /// The face that the library uses when nothing else has a glyph.
    pub fn fallback(&self) -> Option<&FontFace> {
        self.fallback.and_then(|id| self.face(id))
    }

    /// Load a face from bytes. The family name comes from the face when `family` is `None`.
    ///
    /// The face is registered under `requested` as well, which is how a substitute keeps the
    /// name that the document asked for. See `DP-F-03`.
    pub fn load(
        &mut self,
        bytes: Vec<u8>,
        requested: Option<&str>,
        bold: bool,
        italic: bool,
        substituted: bool,
    ) -> Option<u32> {
        if bytes.is_empty() || bytes.len() > MAX_FONT_BYTES {
            return None;
        }
        let face = Face::parse(bytes.as_slice(), 0).ok()?;
        let mut advances = Vec::with_capacity(usize::from(face.number_of_glyphs()));
        let mut bboxes = Vec::with_capacity(usize::from(face.number_of_glyphs()));
        for gid in 0..face.number_of_glyphs() {
            advances.push(face.glyph_hor_advance(GlyphId(gid)).unwrap_or(0));
            let bb = face.glyph_bounding_box(GlyphId(gid));
            bboxes.push(bb.map_or([0.0; 4], |b| {
                [
                    f32::from(b.x_min),
                    f32::from(b.y_min),
                    f32::from(b.x_max),
                    f32::from(b.y_max),
                ]
            }));
        }
        // A face can carry several name records for one id, and a Macintosh record can be
        // undecodable. Take the first record that yields text.
        let family = face
            .names()
            .into_iter()
            .filter(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .find_map(|n| {
                n.to_string()
                    .or_else(|| ascii_mac_name(n.platform_id, n.encoding_id, n.name))
            })
            .unwrap_or_else(|| requested.unwrap_or("unknown").to_string());
        let upem = f32::from(face.units_per_em()).max(1.0);
        // Vertical metrics: `hhea` first, then `OS/2`, then the 0.8/0.2 em fallback of DP-F-42.
        let hhea = face.tables().hhea;
        let (ascender, descender, line_gap) = if hhea.ascender != 0 || hhea.descender != 0 {
            (
                f32::from(hhea.ascender),
                -f32::from(hhea.descender),
                f32::from(hhea.line_gap),
            )
        } else if let Some(os2) = face.tables().os2 {
            (
                f32::from(os2.typographic_ascender()),
                -f32::from(os2.typographic_descender()),
                0.0,
            )
        } else {
            (0.8 * upem, 0.2 * upem, 0.0)
        };
        let glyph_count = face.number_of_glyphs();
        let id = self.faces.len() as u32;
        let requested_name = requested.map_or_else(|| family.clone(), ToString::to_string);
        self.faces.push(FontFace {
            id,
            family: family.clone(),
            bold,
            italic,
            units_per_em: upem,
            ascender,
            descender,
            line_gap,
            substituted,
            requested: requested_name.clone(),
            bytes: Arc::new(bytes),
            advances,
            bboxes,
            glyph_count,
        });
        self.by_key
            .insert((family.to_lowercase(), bold, italic), id);
        if requested_name.to_lowercase() != family.to_lowercase() {
            self.by_key
                .insert((requested_name.to_lowercase(), bold, italic), id);
        }
        if self.fallback.is_none() {
            self.fallback = Some(id);
        }
        Some(id)
    }

    /// Name a face as the fallback.
    pub fn set_fallback(&mut self, id: u32) {
        self.fallback = Some(id);
    }

    /// Register a second name for a face, so a document that asks for the original family finds
    /// the substitute. See `DP-F-03`.
    pub fn alias(&mut self, family: &str, bold: bool, italic: bool, id: u32) {
        if (id as usize) < self.faces.len() {
            self.by_key
                .insert((family.to_lowercase(), bold, italic), id);
        }
    }

    /// True when the face is the style that a caller asked for, so no synthesis is needed.
    ///
    /// A family with only a regular face answers a bold request with its regular face. The painter
    /// then emboldens the glyphs, which is what Word does. See `DP-F-13`.
    pub fn style_matches(&self, id: u32, bold: bool, italic: bool) -> bool {
        self.face(id)
            .is_some_and(|f| f.bold == bold && f.italic == italic)
    }

    /// The face for a family name and a style. It matches the family, then the style, then the
    /// fallback. See `DP-F-02`.
    pub fn resolve(&self, family: &str, bold: bool, italic: bool) -> Option<u32> {
        let key = family.to_lowercase();
        if let Some(id) = self.by_key.get(&(key.clone(), bold, italic)) {
            return Some(*id);
        }
        // Bold before italic before plain, which is how Word falls back inside a family.
        for (b, i) in [
            (bold, italic),
            (bold, false),
            (false, italic),
            (false, false),
        ] {
            if let Some(id) = self.by_key.get(&(key.clone(), b, i)) {
                return Some(*id);
            }
        }
        for family in substitute_candidates(family) {
            for (b, i) in [
                (bold, italic),
                (bold, false),
                (false, italic),
                (false, false),
            ] {
                if let Some(id) = self.by_key.get(&(family.to_ascii_lowercase(), b, i)) {
                    return Some(*id);
                }
            }
        }
        self.fallback
    }

    /// A face that has a glyph for `c`, starting from `preferred`. See `DP-F-15`.
    pub fn face_with_char(&self, preferred: u32, c: char) -> Option<u32> {
        if self.face(preferred).is_some_and(|f| f.has_char(c)) {
            return Some(preferred);
        }
        self.faces.iter().find(|f| f.has_char(c)).map(|f| f.id)
    }

    /// Total bytes held by the faces.
    pub fn bytes_loaded(&self) -> usize {
        self.faces.iter().map(|f| f.bytes.len()).sum()
    }
}

/// Available-family fallbacks, in the order used by the inspected Wordcraft font resolver.
/// Prefer a compatible design and retain serif/monospace families when the first choice is absent.
pub fn substitute_candidates(family: &str) -> &'static [&'static str] {
    match family.to_ascii_lowercase().as_str() {
        "cambria" | "cambria math" | "constantia" | "georgia" | "garamond"
        | "palatino linotype" | "book antiqua" => &[
            "Caladea",
            "Times New Roman",
            "Liberation Serif",
            "Georgia",
            "Noto Serif",
            "DejaVu Serif",
            "Source Serif 4",
            "Tinos",
        ],
        "times new roman" | "times" | "liberation serif" => {
            &["Times New Roman", "Liberation Serif", "Times", "Tinos"]
        }
        "consolas" | "courier" | "courier new" | "lucida console" => &[
            "Courier New",
            "Liberation Mono",
            "Menlo",
            "Consolas",
            "DejaVu Sans Mono",
            "Cousine",
        ],
        "arial" | "liberation sans" | "helvetica" | "segoe ui" | "tahoma" | "verdana"
        | "trebuchet ms" => &[
            "Liberation Sans",
            "Arial",
            "Helvetica",
            "Helvetica Neue",
            "Arimo",
        ],
        "calibri" | "calibri light" | "aptos" | "aptos display" | "carlito" => &[
            "Carlito",
            "Helvetica Neue",
            "Arial",
            "Liberation Sans",
            "Arimo",
        ],
        _ => &[],
    }
}

/// The metric-compatible substitute for a Word face. See `DP-F-03`.
///
/// The table is data. A face with no entry uses the fallback face, and the reader sees one
/// `display.font_substituted` diagnostic.
pub const SUBSTITUTES: &[(&str, &str)] = &[
    ("calibri", "Carlito"),
    ("cambria", "Caladea"),
    ("arial", "Arimo"),
    ("helvetica", "Arimo"),
    ("times new roman", "Tinos"),
    ("times", "Tinos"),
    ("courier new", "Cousine"),
    ("courier", "Cousine"),
    ("aptos", "Carlito"),
    ("aptos display", "Carlito"),
    ("calibri light", "Carlito"),
    ("cambria math", "Carlito"),
    ("segoe ui", "Arimo"),
    ("tahoma", "Arimo"),
    ("verdana", "Arimo"),
    ("georgia", "Tinos"),
    ("garamond", "Tinos"),
    ("palatino linotype", "Tinos"),
    ("book antiqua", "Tinos"),
    ("trebuchet ms", "Arimo"),
    ("franklin gothic book", "Arimo"),
    ("gill sans mt", "Arimo"),
    ("consolas", "Cousine"),
    ("lucida console", "Cousine"),
];

/// The substitute name for a family, or `None` when the table has no entry.
pub fn substitute_for(family: &str) -> Option<&'static str> {
    let key = family.to_lowercase();
    SUBSTITUTES
        .iter()
        .find(|(from, _)| *from == key)
        .map(|(_, to)| *to)
}

/// The result of a shape call.
#[derive(Debug, Clone, Default)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance in font units.
    pub advance: f32,
}

/// Shape a string in one face. The result is in logical order with visual glyph order inside a
/// cluster, which is what the layout needs. See `DP-F-08` and `DP-F-45`.
pub fn shape(face: &FontFace, text: &str, rtl: bool, features: &[&str]) -> ShapedRun {
    // rustybuzz's internal syllable serial is an eight-bit wrapping counter. Very long runs can
    // overflow it in debug builds. Break large runs at whitespace when possible, without splitting
    // grapheme clusters, and retain the original byte offsets in the resulting glyphs.
    const MAX_GRAPHEMES: usize = 128;
    let graphemes = text.grapheme_indices(true).collect::<Vec<_>>();
    if graphemes.len() <= MAX_GRAPHEMES {
        return shape_chunk(face, text, rtl, features);
    }
    let mut ranges = Vec::new();
    let mut start = 0usize;
    while start < graphemes.len() {
        let limit = (start + MAX_GRAPHEMES).min(graphemes.len());
        let end = if limit < graphemes.len() {
            (start + 1..=limit)
                .rev()
                .find(|index| graphemes[*index - 1].1.chars().all(char::is_whitespace))
                .unwrap_or(limit)
        } else {
            limit
        };
        let from = graphemes[start].0;
        let to = graphemes.get(end).map_or(text.len(), |entry| entry.0);
        ranges.push((from, to));
        start = end;
    }
    if rtl {
        ranges.reverse();
    }
    let mut result = ShapedRun::default();
    for (from, to) in ranges {
        let mut chunk = shape_chunk(face, &text[from..to], rtl, features);
        let offset = u32::try_from(from).unwrap_or(u32::MAX);
        for glyph in &mut chunk.glyphs {
            glyph.cluster = glyph.cluster.saturating_add(offset);
        }
        result.advance += chunk.advance;
        result.glyphs.extend(chunk.glyphs);
    }
    result
}

fn shape_chunk(face: &FontFace, text: &str, rtl: bool, features: &[&str]) -> ShapedRun {
    let Ok(parsed) = Face::parse(face_bytes(face), 0) else {
        return fallback_advances(face, text);
    };
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    if rtl {
        buffer.set_direction(rustybuzz::Direction::RightToLeft);
    }
    let rb_face = rustybuzz::Face::from_face(parsed);
    let tag_list: Vec<rustybuzz::Feature> = features
        .iter()
        .filter_map(|f| {
            let (tag, value) = f.strip_prefix('-').map_or((*f, 1), |tag| (tag, 0));
            let mut chars = tag.chars();
            let a = chars.next()?;
            let b = chars.next()?;
            let c = chars.next()?;
            let d = chars.next()?;
            Some(rustybuzz::Feature::new(
                rustybuzz::ttf_parser::Tag::from_bytes(&[a as u8, b as u8, c as u8, d as u8]),
                value,
                ..,
            ))
        })
        .collect();
    let output = rustybuzz::shape(&rb_face, &tag_list, buffer);
    let infos = output.glyph_infos();
    let positions = output.glyph_positions();
    let mut glyphs = Vec::with_capacity(infos.len());
    let mut advance = 0.0f32;
    for (info, pos) in infos.iter().zip(positions.iter()) {
        glyphs.push(ShapedGlyph {
            gid: info.glyph_id as u16,
            advance: pos.x_advance as f32,
            dx: pos.x_offset as f32,
            dy: pos.y_offset as f32,
            cluster: info.cluster,
        });
        advance += pos.x_advance as f32;
    }
    ShapedRun { glyphs, advance }
}

fn face_bytes(face: &FontFace) -> &[u8] {
    face.bytes.as_slice()
}

fn fallback_advances(face: &FontFace, text: &str) -> ShapedRun {
    let mut out = Vec::new();
    let mut advance = 0.0;
    for (i, c) in text.char_indices() {
        let gid = face.glyph_for(c).unwrap_or(0);
        out.push(ShapedGlyph {
            gid,
            advance: face.advance_units(gid),
            dx: 0.0,
            dy: 0.0,
            cluster: i as u32,
        });
        advance += face.advance_units(gid);
    }
    ShapedRun {
        glyphs: out,
        advance,
    }
}

/// A shaper that a test can fix, so a line test needs no font file. See `DP-A-05`.
pub trait Shaper {
    /// Shape one run. `size` is in points and the glyph offsets are in points.
    fn shape_run(&self, face_id: u32, text: &str, size: f32, rtl: bool) -> Vec<ShapedGlyph>;
    /// Shape with the document's kerning choice. Test shapers may retain their fixed metrics.
    fn shape_run_kerning(
        &self,
        face_id: u32,
        text: &str,
        size: f32,
        rtl: bool,
        _kern: bool,
    ) -> Vec<ShapedGlyph> {
        self.shape_run(face_id, text, size, rtl)
    }
    /// The advance of one glyph at a size, in points.
    fn advance(&self, face_id: u32, gid: u16, size: f32) -> f32;
    /// The natural line height of a face at a size.
    fn line_metrics(&self, face_id: u32, size: f32) -> (f32, f32, f32);
    /// The glyph for a character in a face, or 0.
    fn glyph(&self, face_id: u32, c: char) -> u16;
}

/// The real shaper, over a [`FontDb`].
pub struct DbShaper<'a> {
    pub db: &'a FontDb,
}

impl Shaper for DbShaper<'_> {
    fn shape_run(&self, face_id: u32, text: &str, size: f32, rtl: bool) -> Vec<ShapedGlyph> {
        self.shape_run_kerning(face_id, text, size, rtl, true)
    }

    fn shape_run_kerning(
        &self,
        face_id: u32,
        text: &str,
        size: f32,
        rtl: bool,
        kern: bool,
    ) -> Vec<ShapedGlyph> {
        let Some(face) = self.db.face(face_id) else {
            return Vec::new();
        };
        let scale = size / face.units_per_em;
        let mut shaped = shape(face, text, rtl, if kern { &[] } else { &["-kern"] });
        for g in &mut shaped.glyphs {
            g.advance *= scale;
            g.dx *= scale;
            g.dy *= -scale;
        }
        shaped.glyphs
    }

    fn advance(&self, face_id: u32, gid: u16, size: f32) -> f32 {
        self.db.face(face_id).map_or(0.0, |f| f.advance(gid, size))
    }

    fn line_metrics(&self, face_id: u32, size: f32) -> (f32, f32, f32) {
        self.db
            .face(face_id)
            .map_or((0.8 * size, 0.2 * size, 0.0), |f| {
                (
                    f.ascender_pt(size),
                    f.descender_pt(size),
                    f.line_gap / f.units_per_em * size,
                )
            })
    }

    fn glyph(&self, face_id: u32, c: char) -> u16 {
        self.db
            .face(face_id)
            .and_then(|f| f.glyph_for(c))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARLITO: &[u8] = include_bytes!("../../../assets/fonts/subset/Carlito-Regular.ttf");
    const CARLITO_BOLD: &[u8] = include_bytes!("../../../assets/fonts/subset/Carlito-Bold.ttf");

    fn db() -> FontDb {
        let mut db = FontDb::new();
        db.load(CARLITO.to_vec(), Some("Carlito"), false, false, false);
        db.load(CARLITO_BOLD.to_vec(), Some("Carlito"), true, false, false);
        db
    }

    #[test]
    fn a_loaded_face_reports_its_metrics() {
        let db = db();
        assert_eq!(db.len(), 2);
        let face = db.face(0).expect("face 0");
        assert!(face.units_per_em > 0.0);
        assert!(face.ascender > 0.0);
        assert!(face.line_height(11.0) > 10.0);
    }

    #[test]
    fn a_bold_request_finds_the_bold_face() {
        let db = db();
        let regular = db.resolve("Carlito", false, false).expect("regular");
        let bold = db.resolve("Carlito", true, false).expect("bold");
        assert_ne!(regular, bold);
    }

    #[test]
    fn ascii_mac_family_names_are_decoded_without_misreading_other_encodings() {
        use ttf_parser::PlatformId;
        assert_eq!(
            ascii_mac_name(PlatformId::Macintosh, 0, b"Symbol").as_deref(),
            Some("Symbol")
        );
        assert_eq!(ascii_mac_name(PlatformId::Macintosh, 0, &[0x80]), None);
        assert_eq!(ascii_mac_name(PlatformId::Windows, 0, b"Symbol"), None);
        assert_eq!(ascii_mac_name(PlatformId::Macintosh, 1, b"Symbol"), None);
    }

    #[test]
    fn long_runs_shape_without_counter_overflow_and_keep_source_offsets() {
        let db = db();
        let face = db.face(0).expect("regular face");
        let text = " note continuation content".repeat(1800);
        let shaped = shape(face, &text, false, &[]);
        assert!(!shaped.glyphs.is_empty());
        assert!(shaped
            .glyphs
            .iter()
            .any(|glyph| glyph.cluster as usize > text.len() / 2));
        assert!(shaped
            .glyphs
            .iter()
            .all(|glyph| (glyph.cluster as usize) < text.len()));
    }

    #[test]
    fn a_family_name_is_case_insensitive() {
        let db = db();
        assert_eq!(db.resolve("carlito", false, false), Some(0));
        assert_eq!(db.resolve("CARLITO", false, false), Some(0));
    }

    #[test]
    fn an_unknown_family_uses_the_fallback() {
        let db = db();
        assert_eq!(db.resolve("No Such Face", false, false), Some(0));
    }

    #[test]
    fn an_unknown_family_respects_an_explicit_fallback() {
        let mut db = db();
        db.set_fallback(1);
        assert_eq!(db.resolve("No Such Face", false, false), Some(1));
    }

    #[test]
    fn a_substitute_answers_to_the_requested_name() {
        let mut db = FontDb::new();
        db.load(CARLITO.to_vec(), Some("Calibri"), false, false, true);
        let id = db
            .resolve("Calibri", false, false)
            .expect("via requested name");
        let face = db.face(id).expect("face");
        assert!(face.substituted);
        assert_eq!(face.requested, "Calibri");
        assert_eq!(face.family, "Carlito");
    }

    #[test]
    fn the_substitute_table_covers_word_faces() {
        assert_eq!(substitute_for("Calibri"), Some("Carlito"));
        assert_eq!(substitute_for("TIMES NEW ROMAN"), Some("Tinos"));
        assert_eq!(substitute_for("No Such Face"), None);
    }

    #[test]
    fn shaping_advances_forward_for_latin_text() {
        let db = db();
        let face = db.face(0).expect("face");
        let run = shape(face, "Hello", false, &[]);
        assert_eq!(run.glyphs.len(), 5);
        assert!(run.advance > 0.0);
        assert!(run.glyphs.iter().all(|g| g.dx == 0.0));
    }

    #[test]
    fn shaping_keeps_a_cluster_per_character() {
        let db = db();
        let face = db.face(0).expect("face");
        let run = shape(face, "fi", false, &["liga"]);
        // With `liga` on, `fi` may be one ligature glyph with two clusters. Either way the run
        // covers both characters.
        assert!(!run.glyphs.is_empty());
        assert!(run.glyphs.iter().all(|g| g.cluster < 2));
    }

    #[test]
    fn a_face_without_a_glyph_says_so() {
        let db = db();
        let face = db.face(0).expect("face");
        assert!(face.has_char('A'));
        assert!(!face.has_char('\u{10348}'));
    }

    #[test]
    fn an_outline_has_segments() {
        let db = db();
        let face = db.face(0).expect("face");
        let gid = face.glyph_for('A').expect("A");
        let segs = face.outline(gid);
        assert!(segs.len() > 4);
        assert!(matches!(segs.first(), Some(PathSeg::Move(..))));
    }

    #[test]
    fn an_empty_or_oversized_font_is_refused() {
        let mut db = FontDb::new();
        assert_eq!(db.load(Vec::new(), None, false, false, false), None);
        assert_eq!(
            db.load(vec![0u8; MAX_FONT_BYTES + 1], None, false, false, false),
            None
        );
        assert!(db.is_empty());
    }

    #[test]
    fn garbage_bytes_are_refused() {
        let mut db = FontDb::new();
        assert_eq!(db.load(vec![1, 2, 3, 4], None, false, false, false), None);
    }
}
