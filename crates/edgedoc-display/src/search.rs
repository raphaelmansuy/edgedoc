//! Search over the laid-out text.
//!
//! Search reads the text index that each page carries, so it needs no second shaping pass and no
//! access to the model. See `DP-V-26` to `DP-V-30`.

use edgedoc_geom::Rect;

use crate::DisplayDocument;

/// How a query matches.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub match_case: bool,
    pub whole_word: bool,
    /// Fold the common Latin accents, so `cafe` matches `café`.
    pub diacritics: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            match_case: false,
            whole_word: false,
            diacritics: true,
        }
    }
}

/// One hit.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub page: u32,
    /// The rectangles the hit covers. A hit across a line break has more than one.
    pub rects: Vec<Rect>,
    /// The matched text, as the page shows it.
    pub text: String,
}

/// The result of a search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub query: String,
}

impl SearchResults {
    pub fn len(&self) -> usize {
        self.hits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hits.is_empty()
    }

    /// The first page that holds a hit.
    pub fn first_page(&self) -> Option<u32> {
        self.hits.first().map(|h| h.page)
    }
}

/// Fold a string for a case-insensitive match, and strip the common accents.
fn fold(s: &str, options: &SearchOptions) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let c = if options.diacritics {
            accent_fold(c)
        } else {
            c
        };
        if options.match_case {
            out.push(c);
        } else {
            for l in c.to_lowercase() {
                out.push(l);
            }
        }
    }
    out
}

/// Fold one accented Latin letter to its base letter.
fn accent_fold(c: char) -> char {
    match c {
        '\u{00E0}'..='\u{00E5}' | '\u{00C0}'..='\u{00C5}' => 'a',
        '\u{00E8}'..='\u{00EB}' | '\u{00C8}'..='\u{00CB}' => 'e',
        '\u{00EC}'..='\u{00EF}' | '\u{00CC}'..='\u{00CF}' => 'i',
        '\u{00F2}'..='\u{00F6}' | '\u{00D2}'..='\u{00D6}' => 'o',
        '\u{00F9}'..='\u{00FC}' | '\u{00D9}'..='\u{00DC}' => 'u',
        '\u{00FD}' | '\u{00FF}' | '\u{00DD}' => 'y',
        '\u{00F1}' | '\u{00D1}' => 'n',
        '\u{00E7}' | '\u{00C7}' => 'c',
        _ => c,
    }
}

impl DisplayDocument {
    /// Search the document. The call needs a layout, because a hit needs a rectangle.
    pub fn search(&self, query: &str, options: &SearchOptions) -> SearchResults {
        let mut hits = Vec::new();
        if query.is_empty() {
            return SearchResults {
                hits,
                query: query.to_string(),
            };
        }
        let needle = fold(query, options);
        for page in 0..self.pages().len() {
            let Some(display) = self.page(page) else {
                continue;
            };
            // Collect the page text as one string, with a map from each character to its
            // rectangle. A run break inside a word adds no character, which keeps a hit whole.
            let mut text = String::new();
            let mut rects: Vec<Rect> = Vec::new();
            for op in &display.ops {
                if let edgedoc_paint::Op::Glyphs(run) = op {
                    if run.hidden {
                        continue;
                    }
                    let mut x = run.x;
                    for (i, adv) in run.advances.iter().enumerate() {
                        let ch = run.text.chars().nth(i).unwrap_or(' ');
                        text.push(ch);
                        rects.push(Rect::new(
                            x,
                            run.y - run.size * 0.8,
                            (*adv).max(0.5),
                            run.size * 1.15,
                        ));
                        x += adv;
                    }
                }
            }
            let folded = fold(&text, options);
            let mut from = 0usize;
            while from < folded.len() {
                let Some(pos) = folded.get(from..).and_then(|s| s.find(&needle)) else {
                    break;
                };
                let start = from + pos;
                let end = start + needle.len();
                // A whole-word match needs a boundary on both sides.
                if options.whole_word {
                    let before_ok = text
                        .get(..start)
                        .and_then(|s| s.chars().next_back())
                        .map_or(true, |c| !c.is_alphanumeric());
                    let after_ok = text
                        .get(end..)
                        .and_then(|s| s.chars().next())
                        .map_or(true, |c| !c.is_alphanumeric());
                    if !before_ok || !after_ok {
                        from = start + 1;
                        continue;
                    }
                }
                // The rectangles of the matched characters, merged across a line break.
                let mut collected: Vec<Rect> = Vec::new();
                for i in start..end {
                    if let Some(r) = rects.get(i) {
                        match collected.last_mut() {
                            Some(last)
                                if (last.y - r.y).abs() < 1.0
                                    && (last.right() - r.x).abs() < 2.0 =>
                            {
                                last.w = r.right() - last.x;
                            }
                            _ => collected.push(*r),
                        }
                    }
                }
                let shown: String = text
                    .chars()
                    .skip(text.get(..start).map_or(0, |s| s.chars().count()))
                    .take(needle.chars().count())
                    .collect();
                hits.push(SearchHit {
                    page: page as u32,
                    rects: collected,
                    text: if shown.is_empty() {
                        query.to_string()
                    } else {
                        shown
                    },
                });
                from = end.max(start + 1);
                if hits.len() > 20_000 {
                    return SearchResults {
                        hits,
                        query: query.to_string(),
                    };
                }
            }
        }
        SearchResults {
            hits,
            query: query.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fold_removes_an_accent() {
        let o = SearchOptions::default();
        assert_eq!(fold("café", &o), "cafe");
        assert_eq!(fold("CAFÉ", &o), "cafe");
    }

    #[test]
    fn a_case_sensitive_fold_keeps_the_case() {
        let o = SearchOptions {
            match_case: true,
            ..SearchOptions::default()
        };
        assert_ne!(fold("A", &o), fold("a", &o));
    }

    #[test]
    fn an_empty_result_set_reports_itself() {
        let results = SearchResults::default();
        assert!(results.is_empty());
        assert_eq!(results.first_page(), None);
    }
}
