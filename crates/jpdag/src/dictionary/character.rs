//! The character query: how a kanji connects to its components, and what words use it.

use std::sync::LazyLock;

use rusqlite::params;
use serde::Deserialize;

use super::{Dictionary, Error, Word, WordSummary, sql_count, word_rank};
use crate::normalize::{normalize_char, normalize_str};

/// Page of a kanji.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kanji {
    pub character: char,
    /// On readings, then kun readings. Each group is in order of the number of common words that
    /// use the reading, then of all words that use it, then of position.
    pub readings: Vec<CharacterReading>,
    /// English meanings, in position order.
    pub meanings: Vec<String>,
    /// Direct components, in code point order.
    pub components: Vec<CharacterLink>,
    /// Characters that have the kanji as a direct component, in code point order.
    pub used_in: Vec<CharacterLink>,
    /// 1 form for each mutant of the kanji, in code point order.
    pub forms: Vec<Form>,
    /// Full entries of the words with a written form that is exactly the kanji: first the words
    /// whose form at position 1 is the kanji, then the words with the kanji as a later form, such
    /// as みち (道・途・路・径) for 路. Each group is in order of frequency, then of word id.
    pub entries: Vec<Word>,
    /// Other words whose written form at position 1 contains the kanji: `word_limit` words at
    /// most. The matching form of each summary is that written form.
    pub words: KanjiWords,
    /// Number of such words, which can exceed the number in `words`.
    pub word_count: u32,
}

/// Words that contain a kanji, by the position of the kanji in the written form at position 1.
/// Each list is in order of the number of characters of the form that are not kana, then of the
/// length of the form, then of frequency, then of word id: 鳥かご, 鳥目, then 鳥打ち.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KanjiWords {
    /// Words whose form starts with the kanji, for example 私立 for 私.
    pub starting: Vec<WordSummary>,
    /// Words whose form ends with the kanji and does not start with it, for example 公私.
    pub ending: Vec<WordSummary>,
    /// The other words, for example 公私混同.
    pub inside: Vec<WordSummary>,
}

/// A reading of a kanji, from KANJIDIC2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterReading {
    pub kind: ReadingKind,
    /// The reading as KANJIDIC2 writes it: an on reading in katakana, and a kun reading in
    /// hiragana, with a `.` before the okurigana and a `-` for a prefix or a suffix.
    pub text: String,
    /// Number of words that read the kanji with this reading, from the furigana of the words.
    pub word_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReadingKind {
    On,
    Kun,
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

/// The readings, the on readings first. The readings of more common words come first, then the
/// readings of more words.
const READINGS: &str = "
SELECT type AS kind, text, word_count FROM character_reading
WHERE character_id = ?1
ORDER BY type = 'kun', common_word_count DESC, word_count DESC, position";

const MEANINGS: &str = "
SELECT text FROM character_meaning WHERE character_id = ?1 ORDER BY position";

const MUTANTS: &str = "
SELECT mutant_id FROM mutant WHERE base_id = ?1 ORDER BY mutant_id";

/// Words with a written form that is exactly the kanji: those whose form at position 1 is the
/// kanji first, then the others, each in order of frequency. A search-only form (the tag `sK`)
/// does not count, because the entry does not show it.
static ENTRIES: LazyLock<String> = LazyLock::new(|| {
    format!(
        "
SELECT f.word_id
FROM written_form_kanji k JOIN written_form f ON f.id = k.written_form_id
WHERE k.kanji_id = ?1 AND length(f.text) = 1 AND NOT EXISTS (
  SELECT 1 FROM written_form_tag x JOIN tag t ON t.id = x.tag_id
  WHERE x.written_form_id = f.id AND t.name = 'sK')
GROUP BY f.word_id
ORDER BY min(f.position) > 1, rank_band({}), f.word_id",
        word_rank("f.word_id")
    )
});

/// The other words whose written form at position 1 contains the kanji. The limit keeps the most
/// frequent words. They come in order of the length of the form without kana, then of its
/// length, then of frequency.
static WORDS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "
WITH word(word_id, text, band) AS (
  SELECT f.word_id, f.text, rank_band({})
  FROM written_form_kanji k JOIN written_form f ON f.id = k.written_form_id
  WHERE k.kanji_id = ?1 AND f.position = 1 AND length(f.text) > 1
  ORDER BY 3, length_without_kana(f.text), length(f.text), f.word_id
  LIMIT ?2)
SELECT word_id, text FROM word
ORDER BY length_without_kana(text), length(text), band, word_id",
        word_rank("f.word_id")
    )
});

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
struct ReadingRow {
    kind: ReadingKind,
    text: String,
    word_count: u32,
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
        for word_id in self.rows::<u32>(&ENTRIES, [id])? {
            entries.extend(self.word(word_id)?);
        }
        let mut words = KanjiWords::default();
        for row in self.rows::<WordRow>(&WORDS, params![id, sql_count(word_limit)])? {
            let Some(summary) = self.summary(row.word_id, &row.text)? else {
                continue;
            };
            let form = normalize_str(&row.text);
            let list = if form.starts_with(character) {
                &mut words.starting
            } else if form.ends_with(character) {
                &mut words.ending
            } else {
                &mut words.inside
            };
            list.push(summary);
        }
        let readings = self.rows::<ReadingRow>(READINGS, [id])?;
        Ok(Some(Kanji {
            character,
            readings: readings
                .into_iter()
                .map(|row| CharacterReading {
                    kind: row.kind,
                    text: row.text,
                    word_count: row.word_count,
                })
                .collect(),
            meanings: self.rows(MEANINGS, [id])?,
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
