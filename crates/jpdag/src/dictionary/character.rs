//! The character query: how a kanji connects to its components, and what words use it.

use rusqlite::params;
use serde::Deserialize;

use super::{Dictionary, Error, Word, WordSummary, sql_count};
use crate::normalize::normalize_char;

/// Page of a kanji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kanji {
    pub character: char,
    /// Direct components, in code point order.
    pub components: Vec<CharacterLink>,
    /// Characters that have the kanji as a direct component, in code point order.
    pub used_in: Vec<CharacterLink>,
    /// 1 form for each mutant of the kanji, in code point order.
    pub forms: Vec<Form>,
    /// Full entries of the words whose written form at position 1 is exactly the kanji, in word id
    /// order.
    pub entries: Vec<Word>,
    /// Other words whose written form at position 1 contains the kanji, in order of form length,
    /// then word id. The matching form of each summary is that written form.
    pub words: Vec<WordSummary>,
    /// Number of such words, which can exceed the length of `words`.
    pub word_count: u32,
}

/// A link to a character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterLink {
    pub character: char,
    /// Base of the character, if the character is a mutant. A mutant has no page, so its link
    /// goes to the page of the base.
    pub base: Option<char>,
}

/// A mutant of a kanji, for example 亻 for 人.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub mutant: char,
    /// Characters that have the mutant as a direct component, in code point order.
    pub used_in: Vec<CharacterLink>,
}

const LINK: &str = "
SELECT c.id AS character, m.base_id AS base
FROM character c LEFT JOIN mutant m ON m.mutant_id = c.id
WHERE c.id = ?1";

const COMPONENTS: &str = "
SELECT x.component_id AS character, m.base_id AS base
FROM character_component x LEFT JOIN mutant m ON m.mutant_id = x.component_id
WHERE x.character_id = ?1 ORDER BY x.component_id";

const USED_IN: &str = "
SELECT x.character_id AS character, m.base_id AS base
FROM character_component x LEFT JOIN mutant m ON m.mutant_id = x.character_id
WHERE x.component_id = ?1 ORDER BY x.character_id";

const MUTANTS: &str = "
SELECT mutant_id FROM mutant WHERE base_id = ?1 ORDER BY mutant_id";

/// Words whose written form at position 1 is exactly the kanji.
const ENTRIES: &str = "
SELECT f.word_id
FROM written_form_kanji k JOIN written_form f ON f.id = k.written_form_id
WHERE k.kanji_id = ?1 AND f.position = 1 AND length(f.text) = 1
ORDER BY f.word_id";

/// The other words whose written form at position 1 contains the kanji.
const WORDS: &str = "
SELECT f.word_id, f.text
FROM written_form_kanji k JOIN written_form f ON f.id = k.written_form_id
WHERE k.kanji_id = ?1 AND f.position = 1 AND length(f.text) > 1
ORDER BY length(f.text), f.word_id
LIMIT ?2";

const WORD_COUNT: &str = "
SELECT count(*)
FROM written_form_kanji k JOIN written_form f ON f.id = k.written_form_id
WHERE k.kanji_id = ?1 AND f.position = 1 AND length(f.text) > 1";

/// A character and its base, as code points.
#[derive(Deserialize)]
struct LinkRow {
    character: u32,
    base: Option<u32>,
}

#[derive(Deserialize)]
struct WordRow {
    word_id: u32,
    text: String,
}

impl Dictionary {
    /// Gives the page of a character. The result is `None` for a character with no `character`
    /// row, and for a mutant: the page of its base lists the kanji that use it.
    pub fn character(&self, character: char, word_limit: usize) -> Result<Option<Kanji>, Error> {
        let Some(character) = normalize_char(character) else {
            return Ok(None);
        };
        if !matches!(self.link(character)?, Some(link) if link.base.is_none()) {
            return Ok(None);
        }
        let id = u32::from(character);

        let mut forms = Vec::new();
        for mutant in self.rows::<u32>(MUTANTS, [id])? {
            if let Some(character) = char::from_u32(mutant) {
                forms.push(Form {
                    mutant: character,
                    used_in: self.links(USED_IN, mutant)?,
                });
            }
        }
        let mut entries = Vec::new();
        for word_id in self.rows::<u32>(ENTRIES, [id])? {
            entries.extend(self.word(word_id)?);
        }
        let mut words = Vec::new();
        for row in self.rows::<WordRow>(WORDS, params![id, sql_count(word_limit)])? {
            words.extend(self.summary(row.word_id, &row.text)?);
        }
        Ok(Some(Kanji {
            character,
            components: self.links(COMPONENTS, id)?,
            used_in: self.links(USED_IN, id)?,
            forms,
            entries,
            words,
            word_count: self.row(WORD_COUNT, [id])?.unwrap_or(0),
        }))
    }

    /// Gives the link to a character, or `None` for a character with no `character` row.
    pub(super) fn link(&self, character: char) -> Result<Option<CharacterLink>, Error> {
        Ok(self.links(LINK, u32::from(character))?.into_iter().next())
    }

    /// Runs a query that gives the columns `character` and `base`.
    pub(super) fn links(&self, sql: &str, id: u32) -> Result<Vec<CharacterLink>, Error> {
        let rows = self.rows::<LinkRow>(sql, [id])?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(CharacterLink {
                    character: char::from_u32(row.character)?,
                    base: row.base.and_then(char::from_u32),
                })
            })
            .collect())
    }
}
