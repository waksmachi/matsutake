//! The word query: the full entry of 1 word.

use std::sync::LazyLock;

use serde::Deserialize;

use super::{CharacterLink, Dictionary, Error, kana_rank, word_rank};
use crate::kana::morae;
use crate::normalize::normalize_str;

/// Full entry of a word. Each list is in `position` order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub id: u32,
    pub headword: String,
    /// The frequency rank of the word, as the orders of the dictionary rank it: the best of the
    /// ranks of its written forms and readings. `None` if Jiten does not rank it.
    pub rank: Option<u32>,
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
    /// The frequency rank of the word written in this form, if Jiten ranks it.
    pub rank: Option<u32>,
    /// The readings of the form, in position order, each with its furigana for the form.
    pub readings: Vec<FormReading>,
}

/// A reading of a written form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormReading {
    pub text: String,
    /// The form in parts, with the furigana of this reading. A form with a kanji and no furigana
    /// for the reading is 1 part with the whole reading. A form with no kanji and no furigana has
    /// no parts.
    pub furigana: Vec<FuriganaPart>,
}

/// A part of a written form. A part with kana has furigana: 語 with ご in 語学. The other parts,
/// such as the okurigana る in 語る, have none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuriganaPart {
    pub text: String,
    pub kana: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    pub text: String,
    pub no_kanji: bool,
    /// Texts of the written forms that the reading applies to. Empty for all forms.
    pub restricted_to: Vec<String>,
    pub tags: Vec<Tag>,
    /// The reading split into its morae: きょ and う for きょう.
    pub morae: Vec<String>,
    /// The pitch accents of the reading, in order. The first is the usual one. Empty if
    /// `content.db` has none.
    pub accents: Vec<Accent>,
    /// The frequency rank of the word written in kana as this reading, if Jiten ranks it and the
    /// rank counts for the word: not the rank of a reading that other words share, which Jiten
    /// counts for all of them, unless the word is usually in kana.
    pub rank: Option<u32>,
}

/// A pitch accent of a reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accent {
    /// Number of morae before the fall in pitch, or 0 if the pitch does not fall in the word:
    /// はし is 2 as 橋, 1 as 箸, and 0 as 端.
    pub downstep: u32,
    /// The parts of speech that the accent belongs to, as `pos` tags. Empty if the accent belongs
    /// to each part of speech of the word.
    pub parts_of_speech: Vec<Tag>,
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
  (SELECT f.id FROM written_form f
   WHERE f.word_id = word.id AND NOT EXISTS (
     SELECT 1 FROM written_form_tag ft JOIN tag t ON t.id = ft.tag_id
     WHERE ft.written_form_id = f.id AND t.name IN ('iK', 'oK', 'rK', 'sK'))
   ORDER BY f.position LIMIT 1) AS usual_form_id,
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

/// The written forms of a word, without the search-only forms (the tag `sK`). A search finds a
/// word by such a form, but the entry does not show it.
const WRITTEN_FORMS: &str = "
SELECT f.id, f.text, ff.rank FROM written_form f
LEFT JOIN written_form_frequency ff ON ff.written_form_id = f.id
WHERE f.word_id = ?1 AND NOT EXISTS (
  SELECT 1 FROM written_form_tag x JOIN tag t ON t.id = x.tag_id
  WHERE x.written_form_id = f.id AND t.name = 'sK')
ORDER BY f.position";

const WRITTEN_FORM_KANJI: &str = "
SELECT k.kanji_id AS character, m.base_id AS base
FROM written_form_kanji k LEFT JOIN mutant m ON m.mutant_id = k.kanji_id
WHERE k.written_form_id = ?1";

const WRITTEN_FORM_TAGS: &str = "
SELECT t.category, t.name, t.description
FROM written_form_tag x JOIN tag t ON t.id = x.tag_id
WHERE x.written_form_id = ?1 ORDER BY t.id";

/// The readings of a word, without the search-only readings (the tag `sk`), with their kana ranks
/// that count for the word.
static READINGS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "
SELECT r.id, r.text, r.no_kanji, {} AS rank FROM reading r
WHERE r.word_id = ?1 AND NOT EXISTS (
  SELECT 1 FROM reading_tag x JOIN tag t ON t.id = x.tag_id
  WHERE x.reading_id = r.id AND t.name = 'sk')
ORDER BY r.position",
        kana_rank("r.id")
    )
});

/// The frequency rank of the word `?1`.
pub(super) static WORD_RANK: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {}", word_rank("?1")));

const READING_RESTRICTIONS: &str = "
SELECT f.text
FROM reading_restriction x JOIN written_form f ON f.id = x.written_form_id
WHERE x.reading_id = ?1 AND NOT EXISTS (
  SELECT 1 FROM written_form_tag ft JOIN tag t ON t.id = ft.tag_id
  WHERE ft.written_form_id = f.id AND t.name = 'sK')
ORDER BY f.position";

/// The ids of the written forms that a reading is limited to, also the search-only forms. A
/// reading that is limited to search-only forms reads none of the forms of the entry.
const READING_RESTRICTION_IDS: &str = "
SELECT written_form_id FROM reading_restriction WHERE reading_id = ?1";

const READING_TAGS: &str = "
SELECT t.category, t.name, t.description
FROM reading_tag x JOIN tag t ON t.id = x.tag_id
WHERE x.reading_id = ?1 ORDER BY t.id";

const ACCENTS: &str = "
SELECT id, downstep FROM accent WHERE reading_id = ?1 ORDER BY position";

const ACCENT_TAGS: &str = "
SELECT t.category, t.name, t.description
FROM accent_tag x JOIN tag t ON t.id = x.tag_id
WHERE x.accent_id = ?1 ORDER BY t.id";

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
WHERE x.sense_id = ?1 AND NOT EXISTS (
  SELECT 1 FROM written_form_tag ft JOIN tag t ON t.id = ft.tag_id
  WHERE ft.written_form_id = f.id AND t.name = 'sK')
ORDER BY f.position";

const SENSE_READINGS: &str = "
SELECT r.text
FROM sense_reading x JOIN reading r ON r.id = x.reading_id
WHERE x.sense_id = ?1 AND NOT EXISTS (
  SELECT 1 FROM reading_tag rt JOIN tag t ON t.id = rt.tag_id
  WHERE rt.reading_id = r.id AND t.name = 'sk')
ORDER BY r.position";

/// The first reading of a written form, by the rule of `WrittenForm.readings`.
const FIRST_READING: &str = "
SELECT r.id, r.text FROM reading r
WHERE r.word_id = ?1 AND r.no_kanji = 0
  AND NOT EXISTS (
    SELECT 1 FROM reading_tag x JOIN tag t ON t.id = x.tag_id
    WHERE x.reading_id = r.id AND t.name = 'sk')
  AND (NOT EXISTS (SELECT 1 FROM reading_restriction x WHERE x.reading_id = r.id)
    OR EXISTS (
      SELECT 1 FROM reading_restriction x
      WHERE x.reading_id = r.id AND x.written_form_id = ?2))
ORDER BY r.position LIMIT 1";

/// Whether a written form has a kanji.
const HAS_KANJI: &str = "
SELECT EXISTS (SELECT 1 FROM written_form_kanji WHERE written_form_id = ?1)";

const FURIGANA: &str = "
SELECT start, length, kana FROM furigana
WHERE written_form_id = ?1 AND reading_id = ?2 ORDER BY position";

/// Row of [`HEAD`].
#[derive(Deserialize)]
pub(super) struct Head {
    /// First written form without the tag `iK`, `oK`, `rK`, or `sK`.
    usual_form: Option<String>,
    usual_form_id: Option<u32>,
    /// Whether the first sense has the tag `uk`.
    usually_kana: bool,
    /// Reading at position 1.
    pub(super) reading: Option<String>,
    /// Code point of the written form at position 1, if that form is exactly 1 kanji and the
    /// kanji is not a mutant.
    pub(super) kanji: Option<u32>,
}

impl Head {
    /// The id of the written form that is the headword, or `None` if the headword is a reading.
    pub(super) fn headword_form_id(&self) -> Option<u32> {
        self.usual_form_id.filter(|_| !self.usually_kana)
    }

    pub(super) fn headword(&self) -> String {
        let usual_form = self.usual_form.as_ref().filter(|_| !self.usually_kana);
        usual_form
            .or(self.reading.as_ref())
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Deserialize)]
struct FuriganaRow {
    start: usize,
    length: usize,
    kana: String,
}

#[derive(Deserialize)]
struct WrittenFormRow {
    id: u32,
    text: String,
    rank: Option<u32>,
}

#[derive(Deserialize)]
struct ReadingRow {
    id: u32,
    text: String,
    no_kanji: bool,
    rank: Option<u32>,
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
        let mut readings = Vec::new();
        let mut reading_ids = Vec::new();
        // For each reading of a written form: the ids of the forms it is limited to, or `None`
        // for a reading that is not a reading of the forms (`no_kanji`).
        let mut limits: Vec<Option<Vec<u32>>> = Vec::new();
        for row in self.rows::<ReadingRow>(&READINGS, [id])? {
            limits.push(if row.no_kanji {
                None
            } else {
                Some(self.rows(READING_RESTRICTION_IDS, [row.id])?)
            });
            reading_ids.push(row.id);
            let mut accents = Vec::new();
            for (accent_id, downstep) in self.rows::<(u32, u32)>(ACCENTS, [row.id])? {
                accents.push(Accent {
                    downstep,
                    parts_of_speech: self.tags(ACCENT_TAGS, accent_id)?,
                });
            }
            readings.push(Reading {
                morae: morae(&row.text).into_iter().map(str::to_owned).collect(),
                accents,
                text: row.text,
                no_kanji: row.no_kanji,
                restricted_to: self.rows(READING_RESTRICTIONS, [row.id])?,
                tags: self.tags(READING_TAGS, row.id)?,
                rank: row.rank,
            });
        }
        let mut written_forms = Vec::new();
        for row in self.rows::<WrittenFormRow>(WRITTEN_FORMS, [id])? {
            // A reading with no restriction reads each form.
            let of_form: Vec<usize> = (0..readings.len())
                .filter(|&i| {
                    limits[i]
                        .as_ref()
                        .is_some_and(|ids| ids.is_empty() || ids.contains(&row.id))
                })
                .collect();
            let mut form_readings = Vec::new();
            for i in of_form {
                form_readings.push(FormReading {
                    text: readings[i].text.clone(),
                    furigana: self.furigana(
                        row.id,
                        &row.text,
                        reading_ids[i],
                        &readings[i].text,
                    )?,
                });
            }
            written_forms.push(WrittenForm {
                kanji: self.written_form_kanji(&row)?,
                tags: self.tags(WRITTEN_FORM_TAGS, row.id)?,
                rank: row.rank,
                readings: form_readings,
                text: row.text,
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
            rank: self.row::<Option<u32>>(&WORD_RANK, [id])?.flatten(),
            written_forms,
            readings,
            senses,
        }))
    }

    pub(super) fn head(&self, word_id: u32) -> Result<Option<Head>, Error> {
        self.row(HEAD, [word_id])
    }

    /// Gives the furigana of the headword of a word, with the first reading of its form. Empty if
    /// the headword is a reading, or if there are no furigana.
    pub(super) fn headword_furigana(
        &self,
        word_id: u32,
        head: &Head,
    ) -> Result<Vec<FuriganaPart>, Error> {
        let (Some(form_id), Some(text)) = (head.headword_form_id(), &head.usual_form) else {
            return Ok(Vec::new());
        };
        match self.row::<(u32, String)>(FIRST_READING, [word_id, form_id])? {
            Some((reading_id, reading)) => self.furigana(form_id, text, reading_id, &reading),
            None => Ok(Vec::new()),
        }
    }

    /// Splits a written form into parts, with the furigana of a reading. If `content.db` has no
    /// furigana for the form and the reading, a form with a kanji is 1 part with the whole
    /// reading, and a form with no kanji, such as ＡＳＣＩＩ, has no parts.
    fn furigana(
        &self,
        form_id: u32,
        text: &str,
        reading_id: u32,
        reading: &str,
    ) -> Result<Vec<FuriganaPart>, Error> {
        let rows = self.rows::<FuriganaRow>(FURIGANA, [form_id, reading_id])?;
        if rows.is_empty() {
            let has_kanji = self.row::<bool>(HAS_KANJI, [form_id])? == Some(true);
            return Ok(if has_kanji {
                vec![FuriganaPart {
                    text: text.to_owned(),
                    kana: Some(reading.to_owned()),
                }]
            } else {
                Vec::new()
            });
        }
        let characters: Vec<char> = text.chars().collect();
        let slice = |range: std::ops::Range<usize>| -> Option<String> {
            characters.get(range).map(|chars| chars.iter().collect())
        };
        let mut parts = Vec::new();
        let mut next = 0;
        for row in rows {
            let end = row.start + row.length;
            // Ingest checks the ranges. A range outside the form gives no furigana.
            let (Some(before), Some(part)) = (slice(next..row.start), slice(row.start..end)) else {
                return Ok(Vec::new());
            };
            if !before.is_empty() {
                parts.push(FuriganaPart {
                    text: before,
                    kana: None,
                });
            }
            parts.push(FuriganaPart {
                text: part,
                kana: Some(row.kana),
            });
            next = end;
        }
        if !parts.is_empty() && next < characters.len() {
            parts.push(FuriganaPart {
                text: characters[next..].iter().collect(),
                kana: None,
            });
        }
        Ok(parts)
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
