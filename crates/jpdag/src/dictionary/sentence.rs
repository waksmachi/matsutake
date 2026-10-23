//! Sentence bundles: databases of example sentences that the dictionary attaches to `content.db`,
//! as `Docs/bundle-design.md` gives them. Each bundle keeps its own tables in its own schema, so a
//! query runs once in each bundle, and the results merge.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;

use serde::Deserialize;

use super::{Dictionary, Error, FuriganaPart, WordSummary, sql_count};
use crate::schema::{BUNDLE_APPLICATION_ID, BUNDLE_SCHEMA_VERSION};

/// The most bundles that a dictionary attaches. SQLite attaches 10 databases at most.
pub const MAX_BUNDLES: usize = 10;

/// The `bundle` row of an attached bundle.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BundleInfo {
    pub id: String,
    pub title: String,
    pub version: String,
    pub licence: String,
    pub attribution: String,
    pub url: Option<String>,
    /// The page of a sentence, with `{id}` for its id.
    pub sentence_url: Option<String>,
}

/// A bundle that the dictionary attached, and the name of its schema.
#[derive(Debug, Clone)]
pub(super) struct Attached {
    schema: String,
    info: BundleInfo,
}

/// An example sentence of a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    /// The id of the bundle of the sentence.
    pub bundle: String,
    /// The id of the sentence in its bundle.
    pub id: i64,
    pub text: String,
    /// The sentence in parts, with furigana on the parts with kanji. A sentence with no furigana
    /// is 1 part with no kana.
    pub furigana: Vec<FuriganaPart>,
    pub translations: Vec<Translation>,
    pub author: Option<String>,
    pub url: Option<String>,
    /// Whether a person marked the sentence as a good example of the word.
    pub checked: bool,
    /// The characters of `text` that the word covers, if the bundle found them.
    pub word_start: Option<u32>,
    pub word_end: Option<u32>,
    /// The words of the sentence whose characters the bundle found, in the order of the text, the
    /// word of the query among them. A word that overlaps the word before it is left out.
    pub words: Vec<SentenceWord>,
}

/// A word of a sentence: the characters `start..end` of the text, and the word they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentenceWord {
    pub start: u32,
    pub end: u32,
    pub word: WordSummary,
}

#[derive(Deserialize)]
struct WordRow {
    word_id: u32,
    start: u32,
    end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Translation {
    pub language: String,
    pub text: String,
    pub author: Option<String>,
}

#[derive(Deserialize)]
struct SentenceRow {
    id: i64,
    text: String,
    furigana: Option<String>,
    author: Option<String>,
    checked: bool,
    start: Option<u32>,
    end: Option<u32>,
}

/// The parts of a sentence with furigana, as a bundle writes it: `[場所|ば|しょ]とかある？`. A
/// bracket gives a run of kanji, then the reading of each kanji, or 1 reading for all of them; the
/// text outside the brackets has no furigana. Gives `None` for text in another format.
pub fn furigana_parts(furigana: &str) -> Option<Vec<FuriganaPart>> {
    let mut parts = Vec::new();
    let mut rest = furigana;
    while !rest.is_empty() {
        let Some(open) = rest.find('[') else {
            parts.push(FuriganaPart {
                text: rest.to_owned(),
                kana: None,
            });
            break;
        };
        if open > 0 {
            parts.push(FuriganaPart {
                text: rest[..open].to_owned(),
                kana: None,
            });
        }
        let close = open + rest[open..].find(']')?;
        let mut fields = rest[open + 1..close].split('|');
        let base = fields.next()?;
        let readings: Vec<&str> = fields.collect();
        let chars: Vec<char> = base.chars().collect();
        if readings.is_empty() || chars.is_empty() {
            return None;
        }
        let kana = |r: &str| (!r.is_empty()).then(|| r.to_owned());
        if readings.len() == chars.len() {
            for (c, reading) in chars.iter().zip(&readings) {
                parts.push(FuriganaPart {
                    text: c.to_string(),
                    kana: kana(reading),
                });
            }
        } else {
            parts.push(FuriganaPart {
                text: base.to_owned(),
                kana: kana(&readings.concat()),
            });
        }
        rest = &rest[close + 1..];
    }
    Some(parts)
}

impl Dictionary {
    /// Attaches the sentence bundle at `path`, read-only, and gives its `bundle` row. A file that
    /// is not a bundle, a bundle of another schema version, or a second bundle with the same id
    /// gives an error, and stays detached.
    pub fn attach_bundle(&mut self, path: &Path) -> Result<BundleInfo, Error> {
        if self.bundles.len() >= MAX_BUNDLES {
            return Err(Error::TooManyBundles(MAX_BUNDLES));
        }
        let Some(file) = path.to_str() else {
            return Err(Error::NotABundle(path.display().to_string()));
        };
        // The name of the schema is the dictionary's, never the file's.
        let schema = format!("bundle_{}", self.next_bundle);
        self.next_bundle += 1;
        self.conn
            .execute(&format!("ATTACH DATABASE ?1 AS {schema}"), [file])?;
        match self.check_bundle(&schema, path) {
            Ok(info) => {
                self.bundles.push(Attached {
                    schema,
                    info: info.clone(),
                });
                Ok(info)
            }
            Err(error) => {
                self.conn
                    .execute(&format!("DETACH DATABASE {schema}"), [])?;
                Err(error)
            }
        }
    }

    /// The `bundle` row of the bundle attached as `schema`, after the checks of its kind and
    /// version.
    fn check_bundle(&self, schema: &str, path: &Path) -> Result<BundleInfo, Error> {
        let pragma = |name: &str| -> Result<i32, Error> {
            Ok(self
                .conn
                .pragma_query_value(Some(schema), name, |row| row.get(0))?)
        };
        if pragma("application_id")? != BUNDLE_APPLICATION_ID {
            return Err(Error::NotABundle(path.display().to_string()));
        }
        let found = pragma("user_version")?;
        if found != BUNDLE_SCHEMA_VERSION {
            return Err(Error::BundleSchemaVersion {
                found,
                expected: BUNDLE_SCHEMA_VERSION,
            });
        }
        let infos: Vec<BundleInfo> = self.rows(
            &format!(
                "SELECT id, title, version, licence, attribution, url, sentence_url
                 FROM {schema}.bundle LIMIT 2"
            ),
            [],
        )?;
        let [info] = <[BundleInfo; 1]>::try_from(infos)
            .map_err(|_| Error::NotABundle(path.display().to_string()))?;
        if self.bundles.iter().any(|b| b.info.id == info.id) {
            return Err(Error::DuplicateBundle(info.id));
        }
        Ok(info)
    }

    /// The bundles that the dictionary attached, in the order of attachment.
    pub fn bundles(&self) -> Vec<BundleInfo> {
        self.bundles.iter().map(|b| b.info.clone()).collect()
    }

    /// The example sentences of the word `word_id` in all the bundles, `limit` at most after the
    /// first `offset`: first those that a person marked as good examples, then the shorter. A
    /// sentence that has the word twice comes once. Each bundle gives its first `offset + limit`,
    /// so the pages of the merged order do not overlap.
    pub fn sentences(
        &self,
        word_id: u32,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<Sentence>, Error> {
        let mut found = Vec::new();
        for (order, bundle) in self.bundles.iter().enumerate() {
            let s = &bundle.schema;
            let rows: Vec<SentenceRow> = self.rows(
                &format!(
                    "SELECT s.id, s.text, s.furigana, s.author, w.checked, w.start, w.end
                     FROM {s}.sentence_word w JOIN {s}.sentence s ON s.id = w.sentence_id
                     WHERE w.word_id = ?1 AND NOT EXISTS (
                       SELECT 1 FROM {s}.sentence_word o
                       WHERE o.word_id = w.word_id AND o.sentence_id = w.sentence_id
                         AND o.position < w.position)
                     ORDER BY w.checked DESC, length(s.text), s.id
                     LIMIT ?2"
                ),
                (word_id, sql_count(offset.saturating_add(limit))),
            )?;
            for row in rows {
                found.push((order, bundle, row));
            }
        }
        found.sort_by_key(|(order, _, row)| {
            (!row.checked, row.text.chars().count(), *order, row.id)
        });
        let found: Vec<_> = found.into_iter().skip(offset).take(limit).collect();
        let mut sentences = Vec::with_capacity(found.len());
        // A word can come in several sentences: its summary is made once.
        let mut summaries: HashMap<u32, Option<WordSummary>> = HashMap::new();
        for (_, bundle, row) in found {
            let s = &bundle.schema;
            let furigana = row
                .furigana
                .as_deref()
                .and_then(furigana_parts)
                // Furigana that do not spell the sentence are of an older text.
                .filter(|parts| {
                    parts.iter().map(|p| p.text.as_str()).collect::<String>() == row.text
                })
                .unwrap_or_else(|| {
                    vec![FuriganaPart {
                        text: row.text.clone(),
                        kana: None,
                    }]
                });
            let translations = self.rows(
                &format!(
                    "SELECT language, text, author FROM {s}.translation WHERE sentence_id = ?1
                     ORDER BY position"
                ),
                [row.id],
            )?;
            let rows: Vec<WordRow> = self.rows(
                &format!(
                    "SELECT word_id, start, end FROM {s}.sentence_word
                     WHERE sentence_id = ?1 AND start IS NOT NULL AND end IS NOT NULL
                     ORDER BY start, position"
                ),
                [row.id],
            )?;
            let chars: Vec<char> = row.text.chars().collect();
            let mut words = Vec::new();
            let mut covered = 0;
            for w in rows {
                if w.start < covered || w.end <= w.start || w.end as usize > chars.len() {
                    continue;
                }
                let summary = match summaries.entry(w.word_id) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        let form: String = chars[w.start as usize..w.end as usize].iter().collect();
                        entry.insert(self.summary(w.word_id, &form)?)
                    }
                };
                if let Some(summary) = summary {
                    words.push(SentenceWord {
                        start: w.start,
                        end: w.end,
                        word: summary.clone(),
                    });
                    covered = w.end;
                }
            }
            sentences.push(Sentence {
                bundle: bundle.info.id.clone(),
                id: row.id,
                text: row.text,
                furigana,
                translations,
                author: row.author,
                url: bundle
                    .info
                    .sentence_url
                    .as_ref()
                    .map(|url| url.replace("{id}", &row.id.to_string())),
                checked: row.checked,
                word_start: row.start,
                word_end: row.end,
                words,
            });
        }
        Ok(sentences)
    }
}
