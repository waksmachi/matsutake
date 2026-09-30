//! Stage 2: build the entry rows and the tag rows.

use std::collections::{HashMap, HashSet};

use crate::jmdict::{Category, Entry, Jmdict};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenFormRow {
    pub id: u32,
    pub word_id: u32,
    pub position: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingRow {
    pub id: u32,
    pub word_id: u32,
    pub position: u32,
    pub text: String,
    pub no_kanji: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenseRow {
    pub id: u32,
    pub word_id: u32,
    pub position: u32,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossRow {
    pub sense_id: u32,
    pub position: u32,
    pub text: String,
    pub g_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRow {
    pub id: u32,
    pub category: Category,
    pub name: String,
    /// Text that the DTD gives for the entity, or `None` if the DTD does not declare it.
    pub description: Option<String>,
}

/// A `<re_restr>`, `<stagk>`, or `<stagr>` text that names no form of its entry (step 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedRestriction {
    pub seq: u32,
    pub element: &'static str,
    pub text: String,
}

/// Rows of the entry tables and the tag tables. Each pair is (the row id, the other id).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EntryRows {
    pub words: Vec<u32>,
    pub written_forms: Vec<WrittenFormRow>,
    pub readings: Vec<ReadingRow>,
    /// Each row as (reading_id, written_form_id).
    pub reading_restrictions: Vec<(u32, u32)>,
    pub senses: Vec<SenseRow>,
    pub glosses: Vec<GlossRow>,
    /// Each row as (sense_id, written_form_id).
    pub sense_written_forms: Vec<(u32, u32)>,
    /// Each row as (sense_id, reading_id).
    pub sense_readings: Vec<(u32, u32)>,
    pub tags: Vec<TagRow>,
    /// Each row as (written_form_id, tag_id).
    pub written_form_tags: Vec<(u32, u32)>,
    /// Each row as (reading_id, tag_id).
    pub reading_tags: Vec<(u32, u32)>,
    /// Each row as (sense_id, tag_id).
    pub sense_tags: Vec<(u32, u32)>,
    pub skipped_restrictions: Vec<SkippedRestriction>,
}

impl EntryRows {
    /// Tags whose entity the DTD does not declare.
    pub fn undeclared_tags(&self) -> impl Iterator<Item = &TagRow> {
        self.tags.iter().filter(|t| t.description.is_none())
    }
}

/// Adds each pair to a list once.
#[derive(Default)]
struct Links {
    rows: Vec<(u32, u32)>,
    seen: HashSet<(u32, u32)>,
}

impl Links {
    fn add(&mut self, pair: (u32, u32)) {
        if self.seen.insert(pair) {
            self.rows.push(pair);
        }
    }
}

struct Builder<'a> {
    jmdict: &'a Jmdict,
    rows: EntryRows,
    tag_ids: HashMap<(Category, String), u32>,
    reading_restrictions: Links,
    sense_written_forms: Links,
    sense_readings: Links,
    written_form_tags: Links,
    reading_tags: Links,
    sense_tags: Links,
}

impl Builder<'_> {
    fn tag_id(&mut self, category: Category, name: &str) -> u32 {
        let key = (category, name.to_owned());
        if let Some(&id) = self.tag_ids.get(&key) {
            return id;
        }
        let id = self.rows.tags.len() as u32 + 1;
        self.rows.tags.push(TagRow {
            id,
            category,
            name: name.to_owned(),
            description: self.jmdict.entities.get(name).cloned(),
        });
        self.tag_ids.insert(key, id);
        id
    }

    fn skip(&mut self, seq: u32, element: &'static str, text: &str) {
        self.rows.skipped_restrictions.push(SkippedRestriction {
            seq,
            element,
            text: text.to_owned(),
        });
    }

    fn entry(&mut self, entry: &Entry) {
        let seq = entry.seq;
        self.rows.words.push(seq);

        // Steps 1 and 2. The first form with a text answers a restriction with that text.
        let mut kanji_ids: HashMap<&str, u32> = HashMap::new();
        for (i, k) in entry.written_forms.iter().enumerate() {
            let id = self.rows.written_forms.len() as u32 + 1;
            self.rows.written_forms.push(WrittenFormRow {
                id,
                word_id: seq,
                position: i as u32 + 1,
                text: k.text.clone(),
            });
            kanji_ids.entry(&k.text).or_insert(id);
            for tag in &k.tags {
                let tag_id = self.tag_id(Category::KeInf, tag);
                self.written_form_tags.add((id, tag_id));
            }
        }
        let mut reading_ids: HashMap<&str, u32> = HashMap::new();
        for (i, r) in entry.readings.iter().enumerate() {
            let id = self.rows.readings.len() as u32 + 1;
            self.rows.readings.push(ReadingRow {
                id,
                word_id: seq,
                position: i as u32 + 1,
                text: r.text.clone(),
                no_kanji: r.no_kanji,
            });
            reading_ids.entry(&r.text).or_insert(id);
            for tag in &r.tags {
                let tag_id = self.tag_id(Category::ReInf, tag);
                self.reading_tags.add((id, tag_id));
            }
            // Steps 5 and 8.
            for text in &r.restrictions {
                match kanji_ids.get(text.as_str()) {
                    Some(&written_form_id) => self.reading_restrictions.add((id, written_form_id)),
                    None => self.skip(seq, "re_restr", text),
                }
            }
        }

        // Step 3.
        let mut previous_pos: Vec<String> = Vec::new();
        for (i, s) in entry.senses.iter().enumerate() {
            let id = self.rows.senses.len() as u32 + 1;
            self.rows.senses.push(SenseRow {
                id,
                word_id: seq,
                position: i as u32 + 1,
                note: (!s.notes.is_empty()).then(|| s.notes.join("; ")),
            });
            // Step 4.
            for (j, g) in s.glosses.iter().enumerate() {
                self.rows.glosses.push(GlossRow {
                    sense_id: id,
                    position: j as u32 + 1,
                    text: g.text.clone(),
                    g_type: g.g_type.clone(),
                });
            }
            // Steps 6, 7, and 8.
            for text in &s.kanji_restrictions {
                match kanji_ids.get(text.as_str()) {
                    Some(&written_form_id) => self.sense_written_forms.add((id, written_form_id)),
                    None => self.skip(seq, "stagk", text),
                }
            }
            for text in &s.reading_restrictions {
                match reading_ids.get(text.as_str()) {
                    Some(&reading_id) => self.sense_readings.add((id, reading_id)),
                    None => self.skip(seq, "stagr", text),
                }
            }
            // Steps 9 and 10.
            let pos: Vec<String> = s
                .tags
                .iter()
                .filter(|(c, _)| *c == Category::Pos)
                .map(|(_, name)| name.clone())
                .collect();
            if pos.is_empty() {
                for name in &previous_pos {
                    let tag_id = self.tag_id(Category::Pos, name);
                    self.sense_tags.add((id, tag_id));
                }
            } else {
                previous_pos = pos;
            }
            for (category, name) in &s.tags {
                let tag_id = self.tag_id(*category, name);
                self.sense_tags.add((id, tag_id));
            }
        }
    }

    fn finish(mut self) -> EntryRows {
        self.rows.reading_restrictions = self.reading_restrictions.rows;
        self.rows.sense_written_forms = self.sense_written_forms.rows;
        self.rows.sense_readings = self.sense_readings.rows;
        self.rows.written_form_tags = self.written_form_tags.rows;
        self.rows.reading_tags = self.reading_tags.rows;
        self.rows.sense_tags = self.sense_tags.rows;
        self.rows
    }
}

/// Builds the rows of each entry. The ids follow the order of the word id and the position.
pub fn build(jmdict: &Jmdict) -> EntryRows {
    let mut entries: Vec<&Entry> = jmdict.entries.iter().collect();
    entries.sort_by_key(|e| e.seq);
    let mut builder = Builder {
        jmdict,
        rows: EntryRows::default(),
        tag_ids: HashMap::new(),
        reading_restrictions: Links::default(),
        sense_written_forms: Links::default(),
        sense_readings: Links::default(),
        written_form_tags: Links::default(),
        reading_tags: Links::default(),
        sense_tags: Links::default(),
    };
    for entry in entries {
        builder.entry(entry);
    }
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jmdict::{Gloss, Reading, Sense, WrittenForm};

    fn kanji(text: &str, tags: &[&str]) -> WrittenForm {
        WrittenForm {
            text: text.into(),
            tags: tags.iter().map(|t| (*t).into()).collect(),
        }
    }

    fn reading(text: &str) -> Reading {
        Reading {
            text: text.into(),
            ..Reading::default()
        }
    }

    fn sense(tags: &[(Category, &str)]) -> Sense {
        Sense {
            tags: tags.iter().map(|(c, t)| (*c, (*t).into())).collect(),
            ..Sense::default()
        }
    }

    fn jmdict(entries: Vec<Entry>) -> Jmdict {
        Jmdict {
            entries,
            entities: HashMap::from([
                ("n".into(), "noun (common) (futsuumeishi)".into()),
                ("ik".into(), "word containing irregular kana usage".into()),
            ]),
            version: None,
        }
    }

    #[test]
    fn ids_follow_the_word_id_and_the_position() {
        let entry = |seq| Entry {
            seq,
            written_forms: vec![kanji("一", &[]), kanji("壱", &[])],
            readings: vec![reading("いち"), reading("いつ")],
            senses: vec![sense(&[]), sense(&[])],
        };
        // The file order is not the order of the word id.
        let rows = build(&jmdict(vec![entry(20), entry(10)]));
        assert_eq!(rows.words, vec![10, 20]);
        let ids: Vec<(u32, u32, u32)> = rows
            .written_forms
            .iter()
            .map(|k| (k.id, k.word_id, k.position))
            .collect();
        assert_eq!(ids, vec![(1, 10, 1), (2, 10, 2), (3, 20, 1), (4, 20, 2)]);
        let readings: Vec<(u32, u32, u32)> = rows
            .readings
            .iter()
            .map(|r| (r.id, r.word_id, r.position))
            .collect();
        assert_eq!(
            readings,
            vec![(1, 10, 1), (2, 10, 2), (3, 20, 1), (4, 20, 2)]
        );
        let senses: Vec<(u32, u32, u32)> = rows
            .senses
            .iter()
            .map(|s| (s.id, s.word_id, s.position))
            .collect();
        assert_eq!(senses, vec![(1, 10, 1), (2, 10, 2), (3, 20, 1), (4, 20, 2)]);
    }

    #[test]
    fn reading_restriction_links_the_named_written_form() {
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            written_forms: vec![kanji("一寸", &[]), kanji("鳥渡", &[])],
            readings: vec![Reading {
                restrictions: vec!["鳥渡".into()],
                ..reading("ちょっと")
            }],
            senses: vec![sense(&[])],
        }]));
        assert_eq!(rows.reading_restrictions, vec![(1, 2)]);
    }

    #[test]
    fn sense_restrictions_link_the_named_forms() {
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            written_forms: vec![kanji("一寸", &[])],
            readings: vec![reading("ちょっと"), reading("ちょと")],
            senses: vec![Sense {
                kanji_restrictions: vec!["一寸".into()],
                reading_restrictions: vec!["ちょと".into()],
                ..Sense::default()
            }],
        }]));
        assert_eq!(rows.sense_written_forms, vec![(1, 1)]);
        assert_eq!(rows.sense_readings, vec![(1, 2)]);
    }

    #[test]
    fn restriction_that_names_no_form_is_skipped() {
        let rows = build(&jmdict(vec![Entry {
            seq: 42,
            written_forms: vec![kanji("一寸", &[])],
            readings: vec![Reading {
                restrictions: vec!["鳥渡".into()],
                ..reading("ちょっと")
            }],
            senses: vec![sense(&[])],
        }]));
        assert!(rows.reading_restrictions.is_empty());
        assert_eq!(
            rows.skipped_restrictions,
            vec![SkippedRestriction {
                seq: 42,
                element: "re_restr",
                text: "鳥渡".into(),
            }]
        );
    }

    #[test]
    fn sense_without_pos_takes_the_previous_pos() {
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            readings: vec![reading("て")],
            senses: vec![sense(&[(Category::Pos, "n")]), sense(&[])],
            ..Entry::default()
        }]));
        let n = rows.tags.iter().find(|t| t.name == "n").unwrap().id;
        assert_eq!(rows.sense_tags, vec![(1, n), (2, n)]);
    }

    #[test]
    fn same_name_in_two_categories_gives_two_tags() {
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            written_forms: vec![kanji("一寸", &["ik"])],
            readings: vec![Reading {
                tags: vec!["ik".into()],
                ..reading("ちょっと")
            }],
            senses: vec![sense(&[])],
        }]));
        let tags: Vec<(Category, &str)> = rows
            .tags
            .iter()
            .map(|t| (t.category, t.name.as_str()))
            .collect();
        assert_eq!(tags, vec![(Category::KeInf, "ik"), (Category::ReInf, "ik")]);
        assert_eq!(rows.written_form_tags, vec![(1, 1)]);
        assert_eq!(rows.reading_tags, vec![(1, 2)]);
    }

    #[test]
    fn undeclared_entity_gives_a_tag_without_description() {
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            readings: vec![reading("て")],
            senses: vec![sense(&[(Category::Misc, "newtag"), (Category::Pos, "n")])],
            ..Entry::default()
        }]));
        let undeclared: Vec<&str> = rows.undeclared_tags().map(|t| t.name.as_str()).collect();
        assert_eq!(undeclared, vec!["newtag"]);
        let n = rows.tags.iter().find(|t| t.name == "n").unwrap();
        assert_eq!(
            n.description.as_deref(),
            Some("noun (common) (futsuumeishi)")
        );
    }

    #[test]
    fn glosses_get_positions_in_their_sense() {
        let gloss = |text: &str| Gloss {
            text: text.into(),
            g_type: None,
        };
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            readings: vec![reading("て")],
            senses: vec![
                Sense {
                    glosses: vec![gloss("a"), gloss("b"), gloss("c")],
                    ..Sense::default()
                },
                Sense::default(),
            ],
            ..Entry::default()
        }]));
        let glosses: Vec<(u32, u32, &str)> = rows
            .glosses
            .iter()
            .map(|g| (g.sense_id, g.position, g.text.as_str()))
            .collect();
        assert_eq!(glosses, vec![(1, 1, "a"), (1, 2, "b"), (1, 3, "c")]);
        assert_eq!(rows.senses.len(), 2);
    }

    #[test]
    fn sense_notes_are_joined() {
        let noted = |notes: &[&str]| Sense {
            notes: notes.iter().map(|n| (*n).into()).collect(),
            ..Sense::default()
        };
        let rows = build(&jmdict(vec![Entry {
            seq: 1,
            readings: vec![reading("て")],
            senses: vec![noted(&["a"]), noted(&["a", "b"]), noted(&[])],
            ..Entry::default()
        }]));
        let notes: Vec<Option<&str>> = rows.senses.iter().map(|s| s.note.as_deref()).collect();
        assert_eq!(notes, vec![Some("a"), Some("a; b"), None]);
    }
}
