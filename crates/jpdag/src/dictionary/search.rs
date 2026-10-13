//! Search: the words and characters that match what the user typed.

use rusqlite::params;
use serde::Deserialize;

use super::{CharacterLink, Dictionary, Error, sql_count};
use crate::kana;
use crate::normalize::normalize_str;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResults {
    /// Characters of the query, in query order. They do not depend on the offset.
    pub characters: Vec<CharacterLink>,
    /// 1 page of the words with a written form or a reading that starts with a form of the query.
    pub words: Vec<WordSummary>,
}

/// What a result list shows of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSummary {
    pub id: u32,
    /// Usual written form of the word, or its reading at position 1.
    pub headword: String,
    /// Reading at position 1. `None` if the headword is that reading.
    pub reading: Option<String>,
    /// Form that matches, if it is not the headword and not the reading at position 1.
    pub matched_form: Option<String>,
    /// Glosses of the first sense, joined with "; ".
    pub gloss: String,
    /// The character, if the written form at position 1 is exactly 1 kanji and that kanji is not
    /// a mutant. The word then opens the page of that character.
    pub kanji: Option<char>,
}

/// Largest number of forms of a query that the search uses.
const FORMS: usize = 3;

/// Words with a written form or a reading in the range of a form. The parameters 1 to 6 hold the
/// low and high end of the range of each form, or NULL for a form that the query does not have.
/// `CROSS JOIN` keeps `form` in the outer loop, so each range is a lookup in the index on `text`.
///
/// The window function keeps 1 row for each word: its form that the order ranks highest.
const WORDS: &str = "
WITH form(low, high) AS (VALUES (?1, ?2), (?3, ?4), (?5, ?6)),
hit AS (
  SELECT f.word_id, f.text, f.position, f.text = form.low AS is_exact, 0 AS kind
  FROM form CROSS JOIN written_form f ON f.text >= form.low AND f.text < form.high
  UNION ALL
  SELECT r.word_id, r.text, r.position, r.text = form.low, 1
  FROM form CROSS JOIN reading r ON r.text >= form.low AND r.text < form.high
),
ranked AS (
  SELECT word_id, text, is_exact, position = 1 AS is_first, length(text) AS length,
    row_number() OVER (
      PARTITION BY word_id
      ORDER BY is_exact DESC, position = 1 DESC, length(text), kind, position) AS n
  FROM hit
)
SELECT word_id, text FROM ranked WHERE n = 1
ORDER BY is_exact DESC, is_first DESC, length, word_id
LIMIT ?7 OFFSET ?8";

const FIRST_GLOSSES: &str = "
SELECT g.text
FROM sense s JOIN gloss g ON g.sense_id = s.id
WHERE s.word_id = ?1 AND s.position = 1 ORDER BY g.position";

/// A word, and the text of its form that matches.
#[derive(Deserialize)]
struct WordRow {
    word_id: u32,
    text: String,
}

/// Gives the forms of the query that the search uses: the typed, hiragana, and katakana forms
/// that are not empty and differ from each other.
pub(super) fn forms(query: &str) -> Vec<String> {
    let romaji = if query.chars().any(|c| c.is_ascii_alphabetic()) {
        kana::romaji_to_hiragana(query)
    } else {
        None
    };
    let kana = romaji.as_deref().unwrap_or(query);
    let mut forms = Vec::with_capacity(FORMS);
    for form in [
        query.to_owned(),
        kana::to_hiragana(kana),
        kana::to_katakana(kana),
    ] {
        if !form.is_empty() && !forms.contains(&form) {
            forms.push(form);
        }
    }
    forms
}

impl Dictionary {
    /// Gives the characters of the query, and 1 page of the words that match it: `limit` words
    /// at most, after the first `offset` words. A page with fewer than `limit` words is the last.
    pub fn search(&self, query: &str, offset: usize, limit: usize) -> Result<SearchResults, Error> {
        let query = query.trim();
        let mut characters: Vec<CharacterLink> = Vec::new();
        for c in normalize_str(query).chars() {
            if characters.iter().all(|link| link.character != c) {
                characters.extend(self.link(c)?);
            }
        }

        // The range of a form holds each text that starts with the form.
        let forms = forms(query);
        let low = |i: usize| forms.get(i);
        let high = |i: usize| forms.get(i).map(|form| format!("{form}\u{10FFFF}"));
        let rows = if forms.is_empty() {
            Vec::new()
        } else {
            self.rows::<WordRow>(
                WORDS,
                params![
                    low(0),
                    high(0),
                    low(1),
                    high(1),
                    low(2),
                    high(2),
                    sql_count(limit),
                    sql_count(offset)
                ],
            )?
        };
        let mut words = Vec::with_capacity(rows.len());
        for row in rows {
            words.extend(self.summary(row.word_id, &row.text)?);
        }
        Ok(SearchResults { characters, words })
    }

    /// Gives the summary of a word that is in a list because of its form `matched`.
    pub(super) fn summary(&self, id: u32, matched: &str) -> Result<Option<WordSummary>, Error> {
        let Some(head) = self.head(id)? else {
            return Ok(None);
        };
        let headword = head.headword();
        let is_other_form = matched != headword && Some(matched) != head.reading.as_deref();
        Ok(Some(WordSummary {
            id,
            reading: head.reading.filter(|reading| *reading != headword),
            matched_form: is_other_form.then(|| matched.to_owned()),
            gloss: self.rows::<String>(FIRST_GLOSSES, [id])?.join("; "),
            kanji: head.kanji.and_then(char::from_u32),
            headword,
        }))
    }
}
