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

    /// Gives a kanji a KANJIDIC2 reading, with the number of words that use it, and of the common
    /// words.
    fn reading(&self, character: char, position: u32, kind: &str, text: &str, words: [u32; 2]) {
        self.character(character);
        self.execute(
            "INSERT INTO character_reading VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                u32::from(character),
                position,
                kind,
                text,
                words[0],
                words[1]
            ],
        );
    }

    /// Gives a written form the frequency rank of its word in that form.
    fn form_rank(&self, written_form_id: u32, rank: u32) {
        self.execute(
            "INSERT INTO written_form_frequency VALUES (?1, ?2)",
            [written_form_id, rank],
        );
    }

    /// Gives a reading the frequency rank of its word in kana.
    fn kana_rank(&self, reading_id: u32, rank: u32) {
        self.execute(
            "INSERT INTO reading_frequency VALUES (?1, ?2)",
            [reading_id, rank],
        );
    }

    fn meaning(&self, character: char, position: u32, text: &str) {
        self.execute(
            "INSERT INTO character_meaning VALUES (?1, ?2, ?3)",
            params![u32::from(character), position, text],
        );
    }

    /// Gives a written form and a reading furigana, each part as (start, length, kana).
    fn furigana(&self, form_id: u32, reading_id: u32, parts: &[(u32, u32, &str)]) {
        for (position, &(start, length, kana)) in (1..).zip(parts) {
            self.execute(
                "INSERT INTO furigana VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![form_id, reading_id, position, start, length, kana],
            );
        }
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
    fn with_chotto(self) -> Fixture {
        self.with_chotto_at(1)
    }

    /// The word ちょっと, with the id `id`.
    fn with_chotto_at(mut self, id: u32) -> Fixture {
        let word = self.word(id, &["一寸", "鳥渡"], &["ちょっと"], &["a little"]);
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

fn word_ids(words: &[WordSummary]) -> Vec<u32> {
    words.iter().map(|word| word.id).collect()
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
            // No furigana rows: the whole reading over the whole form.
            furigana: vec![FuriganaPart {
                text: "猫背".into(),
                kana: Some("ねこぜ".into()),
            }],
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
    // ごがく is 1, and 0 as a noun. The rows are out of order, and the query sorts them.
    fixture.execute(
        "INSERT INTO accent VALUES (101, ?1, 2, 0), (100, ?1, 1, 1)",
        [word.readings[0]],
    );
    fixture.execute(
        "INSERT INTO accent_tag SELECT 101, id FROM tag WHERE category = 'pos' AND name = 'n'",
        [],
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
                readings: vec![FormReading {
                    text: "ごがく".into(),
                    // No furigana rows: the whole reading over the whole form.
                    furigana: vec![FuriganaPart {
                        text: "語学".into(),
                        kana: Some("ごがく".into()),
                    }],
                }],
            },
            WrittenForm {
                text: "攵語".into(),
                kanji: vec![mutant('攵', '攴'), kanji('語')],
                tags: vec![tag("ke_inf", "rK")],
                readings: vec![],
            },
        ],
        readings: vec![
            Reading {
                text: "ごがく".into(),
                no_kanji: false,
                restricted_to: vec!["語学".into()],
                tags: vec![],
                morae: vec!["ご".into(), "が".into(), "く".into()],
                accents: vec![
                    Accent {
                        downstep: 1,
                        parts_of_speech: vec![],
                    },
                    Accent {
                        downstep: 0,
                        parts_of_speech: vec![tag("pos", "n")],
                    },
                ],
            },
            Reading {
                text: "ゴガク".into(),
                no_kanji: true,
                restricted_to: vec![],
                tags: vec![tag("re_inf", "ik")],
                morae: vec!["ゴ".into(), "ガ".into(), "ク".into()],
                accents: vec![],
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
    assert_eq!(bird.words, KanjiWords::default());
    assert_eq!(bird.word_count, 0);

    // The summary names the written form that holds the kanji, because the headword does not.
    let measure = page(&dictionary, '寸');
    assert_eq!(measure.word_count, 1);
    assert_eq!(measure.words.ending[0].headword, "ちょっと");
    assert_eq!(
        measure.words.ending[0].matched_form.as_deref(),
        Some("一寸")
    );
}

#[test]
fn kanji_word_count_exceeds_the_limit() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["語り手"], &["かたりて"], &["narrator"]);
    fixture.word(2, &["語源"], &["ごげん"], &["origin of a word"]);
    fixture.word(3, &["語学"], &["ごがく"], &["study of languages"]);
    let page = fixture.open().character('語', 2).unwrap().unwrap();
    assert_eq!(word_ids(&page.words.starting), [2, 3]);
    assert_eq!(page.word_count, 3);
}

#[test]
fn kanji_words_are_split_by_the_position_of_the_kanji() {
    let mut fixture = Fixture::new();
    fixture.word(
        1,
        &["公私混同"],
        &["こうしこんどう"],
        &["mixing work and private matters"],
    );
    fixture.word(2, &["公私"], &["こうし"], &["public and private"]);
    fixture.word(3, &["私立"], &["しりつ"], &["private"]);
    fixture.word(4, &["私鉄"], &["してつ"], &["private railway"]);
    fixture.word(5, &["私私"], &["しし"], &["an invented word"]);
    let words = page(&fixture.open(), '私').words;
    assert_eq!(word_ids(&words.starting), [3, 4, 5]);
    assert_eq!(word_ids(&words.ending), [2]);
    assert_eq!(word_ids(&words.inside), [1]);
}

#[test]
fn kanji_words_do_not_count_kana_in_their_length() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["鳥打ち"], &["とりうち"], &["bird hunting"]);
    fixture.word(2, &["鳥目"], &["とりめ"], &["night blindness"]);
    fixture.word(3, &["鳥かご"], &["とりかご"], &["birdcage"]);
    fixture.word(4, &["鳥っぽい"], &["とりっぽい"], &["birdlike"]);
    let words = page(&fixture.open(), '鳥').words;
    // 1 character that is not kana (鳥かご, then the longer 鳥っぽい), then 2 (鳥目, then 鳥打ち).
    assert_eq!(word_ids(&words.starting), [3, 4, 2, 1]);
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
        page.words.starting,
        [WordSummary {
            id: 1,
            headword: "私立".into(),
            reading: Some("しりつ".into()),
            matched_form: None,
            gloss: "private".into(),
            kanji: None,
            // No furigana rows: the whole reading over the whole form.
            furigana: vec![FuriganaPart {
                text: "私立".into(),
                kana: Some("しりつ".into()),
            }],
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

#[test]
fn kanji_has_its_readings_and_meanings() {
    let fixture = Fixture::new();
    // More common words use い.きる than なま, though more words in all use なま.
    fixture.reading('生', 1, "on", "セイ", [1396, 300]);
    fixture.reading('生', 2, "on", "ショウ", [205, 90]);
    fixture.reading('生', 3, "kun", "い.きる", [63, 40]);
    fixture.reading('生', 4, "kun", "なま", [138, 25]);
    fixture.reading('生', 5, "on", "ソウ", [0, 0]);
    fixture.meaning('生', 1, "life");
    fixture.meaning('生', 2, "genuine");
    let page = page(&fixture.open(), '生');
    let readings: Vec<(ReadingKind, &str, u32)> = page
        .readings
        .iter()
        .map(|r| (r.kind, r.text.as_str(), r.word_count))
        .collect();
    assert_eq!(
        readings,
        [
            (ReadingKind::On, "セイ", 1396),
            (ReadingKind::On, "ショウ", 205),
            (ReadingKind::On, "ソウ", 0),
            (ReadingKind::Kun, "い.きる", 63),
            (ReadingKind::Kun, "なま", 138),
        ]
    );
    assert_eq!(page.meanings, ["life", "genuine"]);
}

#[test]
fn word_hides_search_only_forms() {
    let mut fixture = Fixture::new();
    let word = fixture.word(1, &["私", "厶"], &["わたし", "ワタシ"], &["I"]);
    fixture.tag("written_form", word.written_forms[1], "ke_inf", "sK");
    fixture.tag("reading", word.readings[1], "re_inf", "sk");
    fixture.execute(
        "INSERT INTO sense_written_form VALUES (?1, ?2)",
        [word.sense, word.written_forms[1]],
    );
    let dictionary = fixture.open();

    let entry = dictionary.word(1).unwrap().unwrap();
    let forms: Vec<&str> = entry
        .written_forms
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    let readings: Vec<&str> = entry.readings.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(forms, ["私"]);
    assert_eq!(readings, ["わたし"]);
    assert!(entry.senses[0].written_forms.is_empty());
    // The search still finds the word by the search-only reading.
    assert_eq!(found(&dictionary, "ワタシ"), [1]);
}

#[test]
fn written_forms_name_their_readings() {
    let mut fixture = Fixture::new();
    let word = fixture.word(
        1,
        &["土塊", "土くれ", "土䰟"],
        &["つちくれ", "どかい", "ツチクレ", "どこん"],
        &["clod"],
    );
    let restrict = |fixture: &Fixture, reading: usize, form: usize| {
        fixture.execute(
            "INSERT INTO reading_restriction VALUES (?1, ?2)",
            [word.readings[reading], word.written_forms[form]],
        );
    };
    restrict(&fixture, 1, 0);
    // どこん reads only a search-only form, so it reads no form of the entry.
    fixture.tag("written_form", word.written_forms[2], "ke_inf", "sK");
    restrict(&fixture, 3, 2);
    fixture.execute(
        "UPDATE reading SET no_kanji = 1 WHERE id = ?1",
        [word.readings[2]],
    );

    let entry = fixture.open().word(1).unwrap().unwrap();
    let pairs: Vec<(&str, Vec<&str>)> = entry
        .written_forms
        .iter()
        .map(|f| {
            (
                f.text.as_str(),
                f.readings.iter().map(|r| r.text.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        [
            ("土塊", vec!["つちくれ", "どかい"]),
            ("土くれ", vec!["つちくれ"]),
        ]
    );
}

fn parts(furigana: &[FuriganaPart]) -> Vec<(&str, Option<&str>)> {
    furigana
        .iter()
        .map(|part| (part.text.as_str(), part.kana.as_deref()))
        .collect()
}

#[test]
fn each_reading_of_a_form_has_its_furigana() {
    let mut fixture = Fixture::new();
    let word = fixture.word(1, &["土塊", "土くれ"], &["つちくれ", "どかい"], &["clod"]);
    fixture.execute(
        "INSERT INTO reading_restriction VALUES (?1, ?2)",
        [word.readings[1], word.written_forms[0]],
    );
    let (dokai, tsuchikure) = (word.written_forms[0], word.written_forms[1]);
    fixture.furigana(dokai, word.readings[0], &[(0, 2, "つちくれ")]);
    fixture.furigana(dokai, word.readings[1], &[(0, 1, "ど"), (1, 1, "かい")]);
    fixture.furigana(tsuchikure, word.readings[0], &[(0, 1, "つち")]);
    let entry = fixture.open().word(1).unwrap().unwrap();
    let readings = &entry.written_forms[0].readings;
    // A range of characters is 1 part.
    assert_eq!(parts(&readings[0].furigana), [("土塊", Some("つちくれ"))]);
    assert_eq!(
        parts(&readings[1].furigana),
        [("土", Some("ど")), ("塊", Some("かい"))]
    );
    assert_eq!(
        parts(&entry.written_forms[1].readings[0].furigana),
        [("土", Some("つち")), ("くれ", None)]
    );
}

#[test]
fn form_with_kanji_and_no_furigana_has_the_reading_as_1_part() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["逸物"], &["いちもつ"], &["gem"]);
    let dictionary = fixture.open();
    let entry = dictionary.word(1).unwrap().unwrap();
    assert_eq!(
        parts(&entry.written_forms[0].readings[0].furigana),
        [("逸物", Some("いちもつ"))]
    );
    let results = dictionary.search("逸物", 0, 10).unwrap();
    assert_eq!(
        parts(&results.words[0].furigana),
        [("逸物", Some("いちもつ"))]
    );
}

#[test]
fn form_with_no_furigana_has_no_parts() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["ＡＳＣＩＩ"], &["アスキー"], &["ASCII"]);
    let entry = fixture.open().word(1).unwrap().unwrap();
    assert!(entry.written_forms[0].readings[0].furigana.is_empty());
}

#[test]
fn search_gives_the_furigana_of_the_headword() {
    let mut fixture = Fixture::new();
    let word = fixture.word(1, &["語学"], &["ごがく"], &["linguistics"]);
    fixture.furigana(
        word.written_forms[0],
        word.readings[0],
        &[(0, 1, "ご"), (1, 1, "がく")],
    );
    let dictionary = fixture.with_chotto_at(2).open();
    let results = dictionary.search("語学", 0, 10).unwrap();
    assert_eq!(
        parts(&results.words[0].furigana),
        [("語", Some("ご")), ("学", Some("がく"))]
    );
    // A headword that is a reading has no furigana.
    let results = dictionary.search("ちょっと", 0, 10).unwrap();
    assert!(results.words[0].furigana.is_empty());
}

#[test]
fn search_orders_ties_by_frequency() {
    let mut fixture = Fixture::new();
    let edge = fixture.word(1, &["端"], &["はし"], &["edge"]);
    let bridge = fixture.word(2, &["橋"], &["はし"], &["bridge"]);
    fixture.word(3, &["嘴"], &["はし"], &["beak"]);
    let chopsticks = fixture.word(4, &["箸"], &["はし"], &["chopsticks"]);
    let rare = fixture.word(5, &["梯"], &["はし"], &["ladder"]);
    let rarer = fixture.word(6, &["階"], &["はし"], &["stairs"]);
    fixture.form_rank(edge.written_forms[0], 6946);
    fixture.form_rank(bridge.written_forms[0], 3285);
    fixture.form_rank(chopsticks.written_forms[0], 4834);
    // Past COMMON_RANK, ranks are 1 band, so the word id decides.
    fixture.form_rank(rare.written_forms[0], 90000);
    fixture.form_rank(rarer.written_forms[0], 25000);
    // The word with no rank comes last.
    assert_eq!(found(&fixture.open(), "はし"), [2, 4, 1, 5, 6, 3]);
}

#[test]
fn kanji_entries_come_in_order_of_frequency() {
    let mut fixture = Fixture::new();
    let person = fixture.word(1, &["方"], &["かた"], &["person"]);
    let direction = fixture.word(2, &["方"], &["ほう"], &["direction"]);
    fixture.word(3, &["方"], &["がた"], &["plural suffix"]);
    fixture.form_rank(person.written_forms[0], 292);
    fixture.form_rank(direction.written_forms[0], 206);
    let ids: Vec<u32> = page(&fixture.open(), '方')
        .entries
        .iter()
        .map(|word| word.id)
        .collect();
    assert_eq!(ids, [2, 1, 3]);
}

#[test]
fn kanji_words_keep_the_most_frequent_and_order_them_in_a_length() {
    let mut fixture = Fixture::new();
    fixture.word(1, &["鳥目"], &["とりめ"], &["night blindness"]);
    let gate = fixture.word(2, &["鳥居"], &["とりい"], &["torii"]);
    let goosebumps = fixture.word(3, &["鳥肌"], &["とりはだ"], &["goosebumps"]);
    let cage = fixture.word(4, &["鳥かご"], &["とりかご"], &["birdcage"]);
    fixture.form_rank(gate.written_forms[0], 100);
    fixture.form_rank(goosebumps.written_forms[0], 50);
    fixture.form_rank(cage.written_forms[0], 3000);
    let dictionary = fixture.open();
    // Kana do not count in the length, so 鳥かご comes first. Then the words of 2 kanji, in order
    // of frequency, with 鳥目, which has no rank, last.
    let all = dictionary.character('鳥', 10).unwrap().unwrap();
    assert_eq!(word_ids(&all.words.starting), [4, 3, 2, 1]);
    // A limit keeps the most frequent words.
    let two = dictionary.character('鳥', 2).unwrap().unwrap();
    assert_eq!(word_ids(&two.words.starting), [3, 2]);
}

#[test]
fn word_keeps_the_jmdict_order_whatever_the_frequency() {
    let mut fixture = Fixture::new();
    // 禽 is rare in the dramas, and 鶏 has no rank in this word. トリ is more frequent than とり.
    let bird = fixture.word(1, &["鳥", "鶏", "禽"], &["とり", "トリ"], &["bird"]);
    fixture.form_rank(bird.written_forms[0], 3007);
    fixture.form_rank(bird.written_forms[2], 138941);
    fixture.kana_rank(bird.readings[0], 16852);
    fixture.kana_rank(bird.readings[1], 9000);
    // ちょっと is far more frequent in kana, but the sense has no tag uk.
    let chotto = fixture.word(2, &["一寸"], &["ちょっと"], &["a little"]);
    fixture.form_rank(chotto.written_forms[0], 47988);
    fixture.kana_rank(chotto.readings[0], 46);
    let dictionary = fixture.open();
    let entry = dictionary.word(1).unwrap().unwrap();
    let forms: Vec<&str> = entry
        .written_forms
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    let readings: Vec<&str> = entry.readings.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(forms, ["鳥", "鶏", "禽"]);
    assert_eq!(readings, ["とり", "トリ"]);
    assert_eq!(entry.headword, "鳥");
    assert_eq!(dictionary.word(2).unwrap().unwrap().headword, "一寸");
}

#[test]
fn kana_rank_counts_only_for_a_word_in_kana() {
    let mut fixture = Fixture::new();
    // The kana rank of 慣れる also counts なれる of なる, so it does not count without uk.
    let accustomed = fixture.word(1, &["慣れる"], &["なれる"], &["to get used to"]);
    fixture.form_rank(accustomed.written_forms[0], 948);
    fixture.kana_rank(accustomed.readings[0], 402);
    let tame = fixture.word(2, &["馴れる"], &["なれる"], &["to become tame"]);
    fixture.form_rank(tame.written_forms[0], 700);
    // 為れる is usually written in kana, so its kana rank counts.
    let can = fixture.word(3, &["為れる"], &["なれる"], &["to be able to"]);
    fixture.usually_kana(&can);
    fixture.form_rank(can.written_forms[0], 90000);
    fixture.kana_rank(can.readings[0], 300);
    assert_eq!(found(&fixture.open(), "なれる"), [3, 2, 1]);
}

#[test]
fn kana_rank_counts_for_a_reading_of_1_word() {
    let mut fixture = Fixture::new();
    // No other word reads きれい, so its rank in kana counts.
    let pretty = fixture.word(1, &["綺麗"], &["きれい"], &["pretty"]);
    fixture.form_rank(pretty.written_forms[0], 7364);
    fixture.kana_rank(pretty.readings[0], 675);
    let cut = fixture.word(2, &["切れ味"], &["きれあじ"], &["sharpness"]);
    fixture.form_rank(cut.written_forms[0], 5000);
    assert_eq!(found(&fixture.open(), "きれ"), [1, 2]);
}
