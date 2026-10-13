//! Tests of the dictionary. Each test builds a small `content.db` in a temporary file.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use rusqlite::{Connection, params};

use super::search::forms;
use super::*;
use crate::normalize::normalize_str;
use crate::schema::{CREATE_TABLES, SCHEMA_VERSION};

const MOUND: char = '\u{2ED6}'; // ⻖
const CITY: char = '\u{2ECF}'; // ⻏

/// A `content.db` with the rows of 1 test. The file is deleted when the fixture drops.
struct Fixture {
    path: PathBuf,
    conn: Connection,
    next_id: u32,
}

/// Ids of the rows of a word, in position order.
struct WordIds {
    written_forms: Vec<u32>,
    readings: Vec<u32>,
    sense: u32,
}

impl Fixture {
    fn new() -> Fixture {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "jpdag-dictionary-{}-{}.db",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", false).unwrap();
        conn.execute_batch(CREATE_TABLES).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        Fixture {
            path,
            conn,
            next_id: 1,
        }
    }

    fn open(&self) -> Dictionary {
        Dictionary::open(&self.path).unwrap()
    }

    fn id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id - 1
    }

    fn execute(&self, sql: &str, params: impl rusqlite::Params) {
        self.conn.execute(sql, params).unwrap();
    }

    /// Adds a word with 1 sense. Each written form links to its kanji.
    fn word(
        &mut self,
        id: u32,
        written_forms: &[&str],
        readings: &[&str],
        glosses: &[&str],
    ) -> WordIds {
        self.execute("INSERT INTO word VALUES (?1)", [id]);
        let mut ids = WordIds {
            written_forms: Vec::new(),
            readings: Vec::new(),
            sense: 0,
        };
        for (position, text) in (1..).zip(written_forms) {
            let form_id = self.id();
            self.execute(
                "INSERT INTO written_form VALUES (?1, ?2, ?3, ?4)",
                params![form_id, id, position, text],
            );
            for c in normalize_str(text).chars().filter(|&c| c >= '\u{3400}') {
                self.character(c);
                self.execute(
                    "INSERT OR IGNORE INTO written_form_kanji VALUES (?1, ?2)",
                    [form_id, u32::from(c)],
                );
            }
            ids.written_forms.push(form_id);
        }
        for (position, text) in (1..).zip(readings) {
            let reading_id = self.id();
            self.execute(
                "INSERT INTO reading VALUES (?1, ?2, ?3, ?4, 0)",
                params![reading_id, id, position, text],
            );
            ids.readings.push(reading_id);
        }
        ids.sense = self.sense(id, 1, glosses);
        ids
    }

    fn sense(&mut self, word_id: u32, position: u32, glosses: &[&str]) -> u32 {
        let sense_id = self.id();
        self.execute(
            "INSERT INTO sense VALUES (?1, ?2, ?3, NULL)",
            [sense_id, word_id, position],
        );
        for (position, text) in (1..).zip(glosses) {
            self.execute(
                "INSERT INTO gloss VALUES (?1, ?2, ?3, NULL)",
                params![sense_id, position, text],
            );
        }
        sense_id
    }

    /// Gives a tag to a row of `written_form`, `reading`, or `sense`.
    fn tag(&mut self, table: &str, id: u32, category: &str, name: &str) {
        self.execute(
            "INSERT OR IGNORE INTO tag (category, name, description) VALUES (?1, ?2, ?3)",
            [category, name, &format!("description of {name}")],
        );
        self.execute(
            &format!(
                "INSERT INTO {table}_tag
                 SELECT ?1, id FROM tag WHERE category = ?2 AND name = ?3"
            ),
            params![id, category, name],
        );
    }

    /// Gives the first sense of a word the tag `uk`.
    fn usually_kana(&mut self, word: &WordIds) {
        self.tag("sense", word.sense, "misc", "uk");
    }

    fn character(&self, character: char) {
        self.execute(
            "INSERT OR IGNORE INTO character VALUES (?1)",
            [u32::from(character)],
        );
    }

    fn component(&self, character: char, component: char) {
        self.character(character);
        self.character(component);
        self.execute(
            "INSERT INTO character_component VALUES (?1, ?2)",
            [u32::from(character), u32::from(component)],
        );
    }

    fn mutant(&self, mutant: char, base: char) {
        self.character(mutant);
        self.character(base);
        self.execute(
            "INSERT INTO mutant VALUES (?1, ?2)",
            [u32::from(mutant), u32::from(base)],
        );
    }

    /// 休 with the components 亻 and 木, and 亻 with the base 人.
    fn with_rest(self) -> Fixture {
        self.component('休', '亻');
        self.component('休', '木');
        self.mutant('亻', '人');
        self
    }

    /// The word 猫 (ねこ), after the longer word 猫背 (ねこぜ).
    fn with_cats(mut self) -> Fixture {
        self.word(1, &["猫背"], &["ねこぜ"], &["stoop"]);
        self.word(2, &["猫"], &["ねこ"], &["cat"]);
        self
    }

    /// The word ちょっと, with the written forms 一寸 and 鳥渡 and the tag `uk`.
    fn with_chotto(mut self) -> Fixture {
        let word = self.word(1, &["一寸", "鳥渡"], &["ちょっと"], &["a little"]);
        self.usually_kana(&word);
        self
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn kanji(character: char) -> CharacterLink {
    CharacterLink {
        character,
        base: None,
    }
}

fn mutant(character: char, base: char) -> CharacterLink {
    CharacterLink {
        character,
        base: Some(base),
    }
}

fn tag(category: &str, name: &str) -> Tag {
    Tag {
        category: category.into(),
        name: name.into(),
        description: Some(format!("description of {name}")),
    }
}

/// Ids of the words that a search gives.
fn found(dictionary: &Dictionary, query: &str) -> Vec<u32> {
    let results = dictionary.search(query, 0, 10).unwrap();
    results.words.iter().map(|word| word.id).collect()
}

fn page(dictionary: &Dictionary, character: char) -> Kanji {
    dictionary.character(character, 10).unwrap().unwrap()
}

#[test]
fn open_rejects_another_schema_version() {
    let fixture = Fixture::new();
    fixture
        .conn
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    let error = Dictionary::open(&fixture.path).err().unwrap();
    assert!(
        matches!(error, Error::SchemaVersion { found, expected }
            if found == SCHEMA_VERSION + 1 && expected == SCHEMA_VERSION),
        "{error}"
    );
}

#[test]
fn empty_query_gives_no_results() {
    let dictionary = Fixture::new().with_cats().open();
    for query in ["", "  "] {
        let results = dictionary.search(query, 0, 10).unwrap();
        assert!(results.words.is_empty());
        assert!(results.characters.is_empty());
    }
}

#[test]
fn single_kanji_query_gives_its_word_first() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["語学"], &["ごがく"], &["study of languages"]);
    fixture.word(2, &["語"], &["ご"], &["word"]);
    let results = fixture.open().search("語", 0, 10).unwrap();
    let headwords: Vec<&str> = results.words.iter().map(|w| w.headword.as_str()).collect();
    assert_eq!(headwords, ["語", "語学"]);
}

#[test]
fn word_summary_points_at_its_kanji() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["語学"], &["ごがく"], &["study of languages"]);
    fixture.word(2, &["語"], &["ご"], &["word"]);
    let results = fixture.open().search("語", 0, 10).unwrap();
    assert_eq!(results.words[0].kanji, Some('語'));
    assert_eq!(results.words[1].kanji, None);
}

#[test]
fn exact_match_comes_first() {
    let dictionary = Fixture::new().with_cats().open();
    assert_eq!(found(&dictionary, "猫"), [2, 1]);
    assert_eq!(found(&dictionary, "ねこ"), [2, 1]);
}

#[test]
fn search_matches_readings() {
    let dictionary = Fixture::new().with_cats().open();
    let results = dictionary.search("ねこぜ", 0, 10).unwrap();
    assert_eq!(
        results.words,
        [WordSummary {
            id: 1,
            headword: "猫背".into(),
            reading: Some("ねこぜ".into()),
            matched_form: None,
            gloss: "stoop".into(),
            kanji: None,
        }]
    );
}

#[test]
fn search_lists_a_word_once() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["お茶", "お茶っ葉"], &["おちゃ"], &["tea"]);
    assert_eq!(found(&fixture.open(), "お"), [1]);
}

#[test]
fn romaji_query_finds_a_reading() {
    let dictionary = Fixture::new().with_cats().open();
    assert_eq!(found(&dictionary, "neko"), [2, 1]);
    assert_eq!(found(&dictionary, "NEKO"), [2, 1]);
}

#[test]
fn romaji_drops_an_unfinished_syllable() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["学"], &["がく"], &["learning"]);
    fixture.word(2, &["蛾"], &["が"], &["moth"]);
    fixture.word(3, &["楽"], &["らく"], &["comfort"]);
    let dictionary = fixture.open();
    assert_eq!(found(&dictionary, "gak"), [2, 1]);
    assert_eq!(found(&dictionary, "gak"), found(&dictionary, "が"));
    assert_eq!(found(&dictionary, "gaku"), [1]);
}

#[test]
fn letters_that_are_not_romaji_give_no_romaji_form() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["xq線"], &["えっくすきゅーせん"], &["an invented word"]);
    assert_eq!(forms("xq"), ["xq"]);
    assert_eq!(found(&fixture.open(), "xq"), [1]);
}

#[test]
fn hiragana_query_finds_a_katakana_reading() {
    let mut fixture = Fixture::new();
    fixture.word(1, &[], &["コーヒー"], &["coffee"]);
    let dictionary = fixture.open();
    assert_eq!(found(&dictionary, "こーひー"), [1]);
    assert_eq!(found(&dictionary, "ko-hi-"), [1]);
}

#[test]
fn katakana_query_finds_a_hiragana_reading() {
    let dictionary = Fixture::new().with_cats().open();
    assert_eq!(found(&dictionary, "ネコ"), [2, 1]);
}

#[test]
fn search_pages_do_not_overlap() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["語学"], &["ごがく"], &["study of languages"]);
    fixture.word(2, &["語"], &["ご"], &["word"]);
    fixture.word(3, &["語源"], &["ごげん"], &["origin of a word"]);
    fixture.word(4, &["語り手"], &["かたりて"], &["narrator"]);
    let dictionary = fixture.open();
    let words = |offset, limit| dictionary.search("語", offset, limit).unwrap().words;

    let whole = words(0, 4);
    assert_eq!(whole.len(), 4);
    assert_eq!([words(0, 2), words(2, 2)].concat(), whole);
    assert!(words(4, 2).is_empty());
}

#[test]
fn search_gives_the_characters_of_the_query() {
    let mut fixture = Fixture::new();
    fixture.mutant('氵', '水');
    fixture.word(1, &["食べる"], &["たべる"], &["to eat"]);
    fixture.word(2, &["語"], &["ご"], &["word"]);
    let dictionary = fixture.open();

    let results = dictionary.search("氵", 0, 10).unwrap();
    assert_eq!(results.characters, [mutant('氵', '水')]);
    assert!(results.words.is_empty());

    let results = dictionary.search("食べました", 0, 10).unwrap();
    assert_eq!(results.characters, [kanji('食')]);

    let results = dictionary.search("語語", 0, 10).unwrap();
    assert_eq!(results.characters, [kanji('語')]);
}

#[test]
fn search_characters_do_not_depend_on_the_offset() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["語学"], &["ごがく"], &["study of languages"]);
    fixture.word(2, &["語"], &["ご"], &["word"]);
    let dictionary = fixture.open();
    for offset in [0, 1, 2, 5] {
        let results = dictionary.search("語", offset, 1).unwrap();
        assert_eq!(results.characters, [kanji('語')], "offset {offset}");
    }
}

#[test]
fn summary_of_uk_word_has_no_reading() {
    let dictionary = Fixture::new().with_chotto().open();
    let results = dictionary.search("ちょっと", 0, 10).unwrap();
    assert_eq!(results.words[0].headword, "ちょっと");
    assert_eq!(results.words[0].reading, None);
}

#[test]
fn summary_names_the_matched_form() {
    let dictionary = Fixture::new().with_chotto().open();
    let results = dictionary.search("鳥渡", 0, 10).unwrap();
    assert_eq!(results.words[0].headword, "ちょっと");
    assert_eq!(results.words[0].matched_form.as_deref(), Some("鳥渡"));

    let results = dictionary.search("ちょっと", 0, 10).unwrap();
    assert_eq!(results.words[0].matched_form, None);
    // A romaji match names the form as JMdict writes it, and here that is the headword.
    let results = dictionary.search("chotto", 0, 10).unwrap();
    assert_eq!(results.words[0].matched_form, None);
}

#[test]
fn headword_skips_rare_forms() {
    let mut fixture = Fixture::new();
    let word = fixture.word(1, &["誤", "正"], &["せい"], &["correct"]);
    fixture.tag("written_form", word.written_forms[0], "ke_inf", "rK");
    let dictionary = fixture.open();
    assert_eq!(dictionary.word(1).unwrap().unwrap().headword, "正");

    let results = dictionary.search("誤", 0, 10).unwrap();
    assert_eq!(results.words[0].headword, "正");
    assert_eq!(results.words[0].reading.as_deref(), Some("せい"));
    assert_eq!(results.words[0].matched_form.as_deref(), Some("誤"));
}

#[test]
fn headword_of_uk_word_is_the_reading() {
    let dictionary = Fixture::new().with_chotto().open();
    assert_eq!(dictionary.word(1).unwrap().unwrap().headword, "ちょっと");
}

#[test]
fn word_keeps_every_part_in_order() {
    let mut fixture = Fixture::new();
    fixture.mutant('攵', '攴');
    let word = fixture.word(
        7,
        &["語学", "攵語"],
        &["ごがく", "ゴガク"],
        &["study", "lit gloss"],
    );
    let second_sense = fixture.sense(7, 2, &["linguistics"]);
    fixture.tag("written_form", word.written_forms[1], "ke_inf", "rK");
    fixture.tag("reading", word.readings[1], "re_inf", "ik");
    fixture.tag("sense", word.sense, "pos", "n");
    fixture.tag("sense", word.sense, "misc", "col");
    fixture.execute(
        "UPDATE reading SET no_kanji = 1 WHERE id = ?1",
        [word.readings[1]],
    );
    fixture.execute(
        "INSERT INTO reading_restriction VALUES (?1, ?2)",
        [word.readings[0], word.written_forms[0]],
    );
    fixture.execute(
        "UPDATE gloss SET type = 'lit' WHERE sense_id = ?1 AND position = 2",
        [word.sense],
    );
    fixture.execute(
        "UPDATE sense SET note = 'a note' WHERE id = ?1",
        [second_sense],
    );
    fixture.execute(
        "INSERT INTO sense_written_form VALUES (?1, ?2)",
        [second_sense, word.written_forms[1]],
    );
    fixture.execute(
        "INSERT INTO sense_reading VALUES (?1, ?2)",
        [second_sense, word.readings[1]],
    );

    let expected = Word {
        id: 7,
        headword: "語学".into(),
        written_forms: vec![
            WrittenForm {
                text: "語学".into(),
                kanji: vec![kanji('語'), kanji('学')],
                tags: vec![],
            },
            WrittenForm {
                text: "攵語".into(),
                kanji: vec![mutant('攵', '攴'), kanji('語')],
                tags: vec![tag("ke_inf", "rK")],
            },
        ],
        readings: vec![
            Reading {
                text: "ごがく".into(),
                no_kanji: false,
                restricted_to: vec!["語学".into()],
                tags: vec![],
            },
            Reading {
                text: "ゴガク".into(),
                no_kanji: true,
                restricted_to: vec![],
                tags: vec![tag("re_inf", "ik")],
            },
        ],
        senses: vec![
            Sense {
                glosses: vec![
                    Gloss {
                        text: "study".into(),
                        gloss_type: None,
                    },
                    Gloss {
                        text: "lit gloss".into(),
                        gloss_type: Some("lit".into()),
                    },
                ],
                note: None,
                tags: vec![tag("pos", "n"), tag("misc", "col")],
                written_forms: vec![],
                readings: vec![],
            },
            Sense {
                glosses: vec![Gloss {
                    text: "linguistics".into(),
                    gloss_type: None,
                }],
                note: Some("a note".into()),
                tags: vec![],
                written_forms: vec!["攵語".into()],
                readings: vec!["ゴガク".into()],
            },
        ],
    };
    assert_eq!(fixture.open().word(7).unwrap(), Some(expected));
}

#[test]
fn unknown_word_gives_none() {
    let dictionary = Fixture::new().with_cats().open();
    assert_eq!(dictionary.word(3).unwrap(), None);
}

#[test]
fn kanji_links_its_components_both_ways() {
    let dictionary = Fixture::new().with_rest().open();
    let components: Vec<char> = page(&dictionary, '休')
        .components
        .iter()
        .map(|link| link.character)
        .collect();
    assert_eq!(components, ['亻', '木']);
    assert_eq!(page(&dictionary, '木').used_in, [kanji('休')]);
}

#[test]
fn link_to_a_mutant_names_its_base() {
    let dictionary = Fixture::new().with_rest().open();
    assert_eq!(
        page(&dictionary, '休').components,
        [mutant('亻', '人'), kanji('木')]
    );
}

#[test]
fn mutant_has_no_page() {
    let dictionary = Fixture::new().with_rest().open();
    assert_eq!(dictionary.character('亻', 10).unwrap(), None);
    assert_eq!(page(&dictionary, '人').character, '人');
    // 語 has no `character` row.
    assert_eq!(dictionary.character('語', 10).unwrap(), None);
}

#[test]
fn kanji_lists_the_kanji_of_its_forms() {
    let dictionary = Fixture::new().with_rest().open();
    let person = page(&dictionary, '人');
    assert_eq!(
        person.forms,
        [Form {
            mutant: '亻',
            used_in: vec![kanji('休')],
        }]
    );
    assert!(person.used_in.is_empty());
    assert!(page(&dictionary, '木').forms.is_empty());
}

#[test]
fn mound_and_city_are_two_forms() {
    let fixture = Fixture::new();
    fixture.component('院', MOUND);
    fixture.component('部', CITY);
    fixture.mutant(MOUND, '阜');
    fixture.mutant(CITY, '邑');
    let dictionary = fixture.open();

    assert_eq!(page(&dictionary, '院').components, [mutant(MOUND, '阜')]);
    assert_eq!(page(&dictionary, '部').components, [mutant(CITY, '邑')]);
    assert_eq!(
        page(&dictionary, '阜').forms,
        [Form {
            mutant: MOUND,
            used_in: vec![kanji('院')],
        }]
    );
    assert_eq!(
        page(&dictionary, '邑').forms,
        [Form {
            mutant: CITY,
            used_in: vec![kanji('部')],
        }]
    );
}

#[test]
fn word_of_a_mutant_opens_a_word_page() {
    let mut fixture = Fixture::new();
    fixture.mutant('攵', '攴');
    fixture.word(1, &["攵"], &["ぼくづくり"], &["kanji radical 66"]);
    let dictionary = fixture.open();

    let results = dictionary.search("攵", 0, 10).unwrap();
    assert_eq!(results.words[0].kanji, None);
    let word = dictionary.word(1).unwrap().unwrap();
    assert_eq!(word.written_forms[0].kanji, [mutant('攵', '攴')]);
}

#[test]
fn kanji_words_use_the_first_form() {
    let dictionary = Fixture::new().with_chotto().open();
    let bird = page(&dictionary, '鳥');
    assert!(bird.words.is_empty());
    assert_eq!(bird.word_count, 0);

    // The summary names the written form that holds the kanji, because the headword does not.
    let measure = page(&dictionary, '寸');
    assert_eq!(measure.word_count, 1);
    assert_eq!(measure.words[0].headword, "ちょっと");
    assert_eq!(measure.words[0].matched_form.as_deref(), Some("一寸"));
}

#[test]
fn kanji_word_count_exceeds_the_limit() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["語り手"], &["かたりて"], &["narrator"]);
    fixture.word(2, &["語源"], &["ごげん"], &["origin of a word"]);
    fixture.word(3, &["語学"], &["ごがく"], &["study of languages"]);
    let page = fixture.open().character('語', 2).unwrap().unwrap();
    let ids: Vec<u32> = page.words.iter().map(|word| word.id).collect();
    assert_eq!(ids, [2, 3]);
    assert_eq!(page.word_count, 3);
}

#[test]
fn kanji_includes_its_word_entries() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["私立"], &["しりつ"], &["private"]);
    fixture.word(2, &["私"], &["わたし"], &["I", "me"]);
    fixture.word(3, &["私"], &["し"], &["private affairs"]);
    let dictionary = fixture.open();

    let page = page(&dictionary, '私');
    assert_eq!(
        page.entries,
        [
            dictionary.word(2).unwrap().unwrap(),
            dictionary.word(3).unwrap().unwrap()
        ]
    );
    assert_eq!(page.entries[0].senses[0].glosses.len(), 2);
    assert_eq!(
        page.words,
        [WordSummary {
            id: 1,
            headword: "私立".into(),
            reading: Some("しりつ".into()),
            matched_form: None,
            gloss: "private".into(),
            kanji: None,
        }]
    );
    assert_eq!(page.word_count, 1);
}

#[test]
fn character_normalizes_its_input() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["口"], &["くち"], &["mouth"]);
    // ⼝, the Kangxi radical.
    let page = page(&fixture.open(), '\u{2F1D}');
    assert_eq!(page.character, '口');
    assert_eq!(page.entries.len(), 1);
}
