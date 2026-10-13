//! The word query: the full entry of 1 word.

use serde::Deserialize;

use super::{CharacterLink, Dictionary, Error};
use crate::normalize::normalize_str;

/// Full entry of a word. Each list is in `position` order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub id: u32,
    pub headword: String,
    pub written_forms: Vec<WrittenForm>,
    pub readings: Vec<Reading>,
    pub senses: Vec<Sense>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenForm {
    pub text: String,
    /// Kanji of the form, in text order.
    pub kanji: Vec<CharacterLink>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub text: String,
    pub no_kanji: bool,
    /// Texts of the written forms that the reading applies to. Empty for all forms.
    pub restricted_to: Vec<String>,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sense {
    pub glosses: Vec<Gloss>,
    pub note: Option<String>,
    pub tags: Vec<Tag>,
    /// Texts of the written forms that the sense applies to. Empty for all forms.
    pub written_forms: Vec<String>,
    /// Texts of the readings that the sense applies to. Empty for all readings.
    pub readings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gloss {
    pub text: String,
    pub gloss_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub category: String,
    pub name: String,
    pub description: Option<String>,
}

/// What the headword of a word depends on, and what a summary shows beside it. The query gives no
/// row for an unknown word.
const HEAD: &str = "
SELECT
  (SELECT f.text FROM written_form f
   WHERE f.word_id = word.id AND NOT EXISTS (
     SELECT 1 FROM written_form_tag ft JOIN tag t ON t.id = ft.tag_id
     WHERE ft.written_form_id = f.id AND t.name IN ('iK', 'oK', 'rK', 'sK'))
   ORDER BY f.position LIMIT 1) AS usual_form,
  EXISTS (
    SELECT 1 FROM sense s
    JOIN sense_tag st ON st.sense_id = s.id
    JOIN tag t ON t.id = st.tag_id
    WHERE s.word_id = word.id AND s.position = 1 AND t.name = 'uk') AS usually_kana,
  (SELECT r.text FROM reading r WHERE r.word_id = word.id AND r.position = 1) AS reading,
  (SELECT k.kanji_id FROM written_form f
   JOIN written_form_kanji k ON k.written_form_id = f.id
   WHERE f.word_id = word.id AND f.position = 1 AND length(f.text) = 1
     AND NOT EXISTS (SELECT 1 FROM mutant m WHERE m.mutant_id = k.kanji_id)) AS kanji
FROM word WHERE word.id = ?1";

const WRITTEN_FORMS: &str = "
SELECT id, text FROM written_form WHERE word_id = ?1 ORDER BY position";

const WRITTEN_FORM_KANJI: &str = "
SELECT k.kanji_id AS character, m.base_id AS base
FROM written_form_kanji k LEFT JOIN mutant m ON m.mutant_id = k.kanji_id
WHERE k.written_form_id = ?1";

const WRITTEN_FORM_TAGS: &str = "
SELECT t.category, t.name, t.description
FROM written_form_tag x JOIN tag t ON t.id = x.tag_id
WHERE x.written_form_id = ?1 ORDER BY t.id";

const READINGS: &str = "
SELECT id, text, no_kanji FROM reading WHERE word_id = ?1 ORDER BY position";

const READING_RESTRICTIONS: &str = "
SELECT f.text
FROM reading_restriction x JOIN written_form f ON f.id = x.written_form_id
WHERE x.reading_id = ?1 ORDER BY f.position";

const READING_TAGS: &str = "
SELECT t.category, t.name, t.description
FROM reading_tag x JOIN tag t ON t.id = x.tag_id
WHERE x.reading_id = ?1 ORDER BY t.id";

const SENSES: &str = "
SELECT id, note FROM sense WHERE word_id = ?1 ORDER BY position";

const GLOSSES: &str = "
SELECT text, type AS gloss_type FROM gloss WHERE sense_id = ?1 ORDER BY position";

const SENSE_TAGS: &str = "
SELECT t.category, t.name, t.description
FROM sense_tag x JOIN tag t ON t.id = x.tag_id
WHERE x.sense_id = ?1 ORDER BY t.id";

const SENSE_WRITTEN_FORMS: &str = "
SELECT f.text
FROM sense_written_form x JOIN written_form f ON f.id = x.written_form_id
WHERE x.sense_id = ?1 ORDER BY f.position";

const SENSE_READINGS: &str = "
SELECT r.text
FROM sense_reading x JOIN reading r ON r.id = x.reading_id
WHERE x.sense_id = ?1 ORDER BY r.position";

/// Row of [`HEAD`].
#[derive(Deserialize)]
pub(super) struct Head {
    /// First written form without the tag `iK`, `oK`, `rK`, or `sK`.
    usual_form: Option<String>,
    /// Whether the first sense has the tag `uk`.
    usually_kana: bool,
    /// Reading at position 1.
    pub(super) reading: Option<String>,
    /// Code point of the written form at position 1, if that form is exactly 1 kanji and the
    /// kanji is not a mutant.
    pub(super) kanji: Option<u32>,
}

impl Head {
    pub(super) fn headword(&self) -> String {
        let usual_form = self.usual_form.as_ref().filter(|_| !self.usually_kana);
        usual_form
            .or(self.reading.as_ref())
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
struct WrittenFormRow {
    id: u32,
    text: String,
}

#[derive(Deserialize)]
struct ReadingRow {
    id: u32,
    text: String,
    no_kanji: bool,
}

#[derive(Deserialize)]
struct SenseRow {
    id: u32,
    note: Option<String>,
}

#[derive(Deserialize)]
struct GlossRow {
    text: String,
    gloss_type: Option<String>,
}

#[derive(Deserialize)]
struct TagRow {
    category: String,
    name: String,
    description: Option<String>,
}

impl Dictionary {
    /// Gives the full entry of the word `id`, or `None` for an unknown id.
    pub fn word(&self, id: u32) -> Result<Option<Word>, Error> {
        let Some(head) = self.head(id)? else {
            return Ok(None);
        };
        let mut written_forms = Vec::new();
        for row in self.rows::<WrittenFormRow>(WRITTEN_FORMS, [id])? {
            written_forms.push(WrittenForm {
                kanji: self.written_form_kanji(&row)?,
                tags: self.tags(WRITTEN_FORM_TAGS, row.id)?,
                text: row.text,
            });
        }
        let mut readings = Vec::new();
        for row in self.rows::<ReadingRow>(READINGS, [id])? {
            readings.push(Reading {
                text: row.text,
                no_kanji: row.no_kanji,
                restricted_to: self.rows(READING_RESTRICTIONS, [row.id])?,
                tags: self.tags(READING_TAGS, row.id)?,
            });
        }
        let mut senses = Vec::new();
        for row in self.rows::<SenseRow>(SENSES, [id])? {
            let glosses = self.rows::<GlossRow>(GLOSSES, [row.id])?;
            senses.push(Sense {
                glosses: glosses
                    .into_iter()
                    .map(|gloss| Gloss {
                        text: gloss.text,
                        gloss_type: gloss.gloss_type,
                    })
                    .collect(),
                note: row.note,
                tags: self.tags(SENSE_TAGS, row.id)?,
                written_forms: self.rows(SENSE_WRITTEN_FORMS, [row.id])?,
                readings: self.rows(SENSE_READINGS, [row.id])?,
            });
        }
        Ok(Some(Word {
            id,
            headword: head.headword(),
            written_forms,
            readings,
            senses,
        }))
    }

    pub(super) fn head(&self, word_id: u32) -> Result<Option<Head>, Error> {
        self.row(HEAD, [word_id])
    }

    /// Gives the kanji of a written form in text order. `written_form_kanji` has no order column.
    fn written_form_kanji(&self, form: &WrittenFormRow) -> Result<Vec<CharacterLink>, Error> {
        let mut kanji = self.links(WRITTEN_FORM_KANJI, form.id)?;
        let text = normalize_str(&form.text);
        kanji.sort_by_key(|link| {
            let index = text.chars().position(|c| c == link.character);
            (index.unwrap_or(usize::MAX), link.character)
        });
        Ok(kanji)
    }

    fn tags(&self, sql: &str, id: u32) -> Result<Vec<Tag>, Error> {
        let tags = self.rows::<TagRow>(sql, [id])?;
        Ok(tags
            .into_iter()
            .map(|tag| Tag {
                category: tag.category,
                name: tag.name,
                description: tag.description,
            })
            .collect())
    }
}
