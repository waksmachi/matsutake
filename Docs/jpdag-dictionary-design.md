# jpdag: the dictionary component

## 1. Summary

`jpdag::dictionary` serves as an interface for querying the JMDict-derived tables of `content.db`: 
which words and kanji match what the user typed, what a word's entry says, how a kanji connects 
to its components, and what words use it.

## 2. Scope

This design covers `jpdag::dictionary`: opening `content.db`, search (with romaji and kana input),
the word query, and the character query.

`Docs/future-work.md` lists the work that this design leaves out.

## 3. Opening

```rust
impl Dictionary {
    pub fn open(path: &Path) -> Result<Dictionary, Error>;
}
```

`open` opens the file read-only, and checks that `PRAGMA user_version` equals
`jpdag::schema::SCHEMA_VERSION`. A different version gives `Error::SchemaVersion`.

`open` uses `rusqlite`'s bundled SQLite, not the device's. `content.db` uses `STRICT` tables, which
need SQLite 3.37 or later, and older Android versions ship an older SQLite.

### Queries and rows

`jpdag` has no ORM and no query builder:

- Each query is plain SQL in a string, which `rusqlite` runs.
- `serde_rusqlite` maps each result row to a private row struct that derives `Deserialize`, by
  column name.
- The API structs of this design are not the row structs. A function builds each API struct from
  its rows: it turns a code point into a `char`, and it puts the rows of the lists, for example the
  senses of a word, into their `Vec`.

## 4. Search

```rust
pub fn search(&self, query: &str, offset: usize, limit: usize) -> Result<SearchResults, Error>;

pub struct SearchResults {
    pub characters: Vec<CharacterLink>,
    pub words: Vec<WordSummary>,
}

pub struct WordSummary {
    pub id: u32,
    pub headword: String,
    pub reading: Option<String>,
    pub matched_form: Option<String>,
    pub gloss: String,
    pub kanji: Option<char>,
    pub furigana: Vec<FuriganaPart>,
}
```

The query parameter is trimmed. An empty query gives no characters and no words.

### 4.1 Forms of the query

The search builds these forms of the query:

1. The typed form: the query as the user typed it.
2. The romaji form, if the query has ASCII letters: the query with each run of letters converted to
   hiragana.
3. The hiragana form: the romaji form, or the typed form if there is no romaji form, with each
   katakana converted to hiragana.
4. The katakana form: the same, with each hiragana converted to katakana.

The search uses the typed, hiragana, and katakana forms that differ from each other: 3 forms at
most. So `neko`, ねこ, and ネコ all find 猫 by its reading ねこ, and `ko-hi-`, こーひー, and
コーヒー all find コーヒー.

The romaji conversion, in `jpdag::kana`, reads the letters as an input method does. It uses the
romaji table of Mozc, the open-source input method of Google. Each row of the table gives the kana
for a sequence of letters, for example し for `shi` and for `si`. At each position of the query,
the longest row that matches gives the kana. The conversion uses the rows that give kana, and not
the rows for punctuation and symbols.

The conversion:

- ignores case
- reads the Hepburn and the kunrei spellings: `shi` and `si`, `tsu` and `tu`, `chi` and `ti`, `fu`
  and `hu`, `ji` and `zi`
- gives っ for a doubled consonant, ん for `nn`, `n'`, and an `n` before a consonant or at the end,
  and ー for `-`
- reads `nn` as 1 ん in each position. So `onna` gives おんあ, and `onnna` gives おんな.
- drops an unfinished syllable at the end of the query: letters that start a row and do not
  complete it. So `gak` searches for が, `gaku` searches for がく, and `sh` gives an empty form.
- does not change a character that starts no row. So 食べmasu gives 食べます.

If any other letter is not part of a row, the query has no romaji form. A form that is empty is
not used.

The table is `crates/jpdag/data/mozc/romanji-hiragana.tsv`, an exact copy of the file of Mozc. Its
licence is BSD 3-Clause, and `crates/jpdag/data/mozc/` holds the licence text and the commit of
the copy. `jpdag` includes the file when it compiles.

### 4.2 Words

`words` holds the words with a written form or a reading that starts with a form of the query. The
search uses the indexes on `written_form.text` and `reading.text` with a range for each form, `text
>= form AND text < form || U+10FFFF`, not `LIKE`. The words are in this order:

1. words where a written form or reading equals a form of the query
2. words where the matching form is at position 1
3. shorter matching forms
4. word id

A word appears once. If a word matches by more than 1 written form or reading, its matching form is
the one that rules 1 to 3 rank highest.

**Pages.** `offset` skips that number of words in the order, and `limit` caps the number of words.
Consecutive pages do not overlap and leave no gap. A page with fewer than `limit` words is the last
page.

**`WordSummary`** is what a result list shows:

| Field | Value |
| --- | --- |
| `headword` | Usual written form of the word (below) |
| `reading` | Reading at position 1. None if the headword is that reading. |
| `matched_form` | The matching form, if it is not the headword and not the reading at position 1. For example, 鳥渡 gives the word ちょっと with the matched form 鳥渡. For a romaji or kana match, the text is the form as JMdict writes it. |
| `gloss` | Glosses of the first sense, joined with "; " |
| `furigana` | The headword in parts, with the furigana of the first reading of its form (section 5). Empty if the headword is a reading, or a form with no kanji and no furigana for the reading. |
| `kanji` | The character, if the word's written form at position 1 is exactly 1 kanji and that kanji is not a mutant. The app then opens the page of that character (section 6), not a word page. |

**The headword** is the first written form without the tag `iK`, `oK`, `rK`, or `sK`. If the first
sense has the tag `uk`, or no written form qualifies, the headword is the reading at position 1.

### 4.3 Characters

`characters` holds each code point of the typed form, after normalization, that has a `character`
row: in query order, with each character once. Each is a `CharacterLink` (section 6). `characters`
does not depend on `offset`.

## 5. Word

```rust
pub fn word(&self, id: u32) -> Result<Option<Word>, Error>;

pub struct Word {
    pub id: u32,
    pub headword: String,
    pub written_forms: Vec<WrittenForm>,
    pub readings: Vec<Reading>,
    pub senses: Vec<Sense>,
}

pub struct WrittenForm {
    pub text: String,
    pub kanji: Vec<CharacterLink>,
    pub tags: Vec<Tag>,
    pub readings: Vec<FormReading>,
}

pub struct FormReading {
    pub text: String,
    pub furigana: Vec<FuriganaPart>,
}

pub struct FuriganaPart {
    pub text: String,
    pub kana: Option<String>,
}

pub struct Reading {
    pub text: String,
    pub no_kanji: bool,
    pub restricted_to: Vec<String>,
    pub tags: Vec<Tag>,
    pub morae: Vec<String>,
    pub accents: Vec<Accent>,
}

pub struct Accent {
    pub downstep: u32,
    pub parts_of_speech: Vec<Tag>,
}

pub struct Sense {
    pub glosses: Vec<Gloss>,
    pub note: Option<String>,
    pub tags: Vec<Tag>,
    pub written_forms: Vec<String>,
    pub readings: Vec<String>,
}

pub struct Gloss {
    pub text: String,
    pub gloss_type: Option<String>,
}

pub struct Tag {
    pub category: String,
    pub name: String,
    pub description: Option<String>,
}
```

Each list is in `position` order, the order of JMdict. `WrittenForm.kanji` comes from
`written_form_kanji`, in text order, each as a `CharacterLink` (section 6).

`WrittenForm.readings` holds the readings of the form, in position order. A reading
reads a form if it is not `no_kanji`, and it has no restriction or a restriction to that form.
So the page can show each form with its readings: 土塊 with つちくれ and どかい, and 土くれ with
つちくれ alone, because どかい is limited to 土塊. A reading that reads no form, for example トリ
of 鳥, is in `Word.readings` only.

`FormReading.furigana` is the form in parts, with the furigana of that reading, from the
`furigana` table: 語る with かたる gives 語 with かた and る with none, and 土塊 gives 土塊 with
つちくれ (1 part for both kanji) and 土 with ど and 塊 with かい. If `content.db` has no furigana
for the form and the reading, a form with a kanji is 1 part with the whole reading, for example
逸物 with いちもつ, and a form with no kanji, for example ＡＳＣＩＩ, has no parts.

`Reading.morae` splits the reading with `jpdag::kana::morae`: きょう gives きょ and う. A page
draws the pitch of each mora from them. `Reading.accents` holds the pitch accents of the reading,
from the `accent` table, in position order, so the usual accent comes first. `Accent.downstep` is
the number of morae before the pitch falls, or 0 if it does not fall in the word. A reading with
no accent in `content.db` has none, and the page shows no accent for it.
`Accent.parts_of_speech` holds the `pos` tags of `accent_tag`: ２つ is 0 as an adverb (`adv`) and
3 as a noun (`n`). An accent with no tag belongs to each part of speech.

The entry leaves out the search-only forms: the written forms with the tag `sK` and the readings
with the tag `sk`, and the restrictions that name them. JMdict lists these forms so that a search
finds the word by them, for example ワタシ for 私, and the search of section 4 still does. `Reading.restricted_to`, `Sense.written_forms`, and
`Sense.readings` hold the texts of the forms that the restrictions name. An empty list means "all
forms". An unknown id gives `None`.

## 6. Characters

```rust
pub fn character(&self, character: char, word_limit: usize) -> Result<Option<Kanji>, Error>;

pub struct Kanji {
    pub character: char,
    pub readings: Vec<CharacterReading>,
    pub meanings: Vec<String>,
    pub components: Vec<CharacterLink>,
    pub used_in: Vec<CharacterLink>,
    pub forms: Vec<Form>,
    pub entries: Vec<Word>,
    pub words: KanjiWords,
    pub word_count: u32,
}

pub struct CharacterReading {
    pub kind: ReadingKind,
    pub text: String,
    pub word_count: u32,
}

pub enum ReadingKind {
    On,
    Kun,
}

pub struct KanjiWords {
    pub starting: Vec<WordSummary>,
    pub ending: Vec<WordSummary>,
    pub inside: Vec<WordSummary>,
}

pub struct CharacterLink {
    pub character: char,
    pub base: Option<char>,
}

pub struct Form {
    pub mutant: char,
    pub used_in: Vec<CharacterLink>,
}
```

`character` normalizes its argument first. It gives `None` for a character with no `character` row,
and for a mutant. A mutant has no page: most kanji use the mutant and not its base, so the page of
the base lists them in `forms`.

| Field | Value |
| --- | --- |
| `readings` | The KANJIDIC2 readings of the character: the on readings, then the kun readings. Each group is in order of `word_count`, highest first, then position. |
| `CharacterReading.text` | The reading as KANJIDIC2 writes it: an on reading in katakana, and a kun reading in hiragana, with a `.` before the okurigana and a `-` for a prefix or a suffix |
| `CharacterReading.word_count` | `character_reading.word_count`: the number of words that read the character with the reading |
| `meanings` | The English meanings of the character, from KANJIDIC2, in position order |
| `CharacterLink.base` | Base of the character, if the character is a mutant. `None` for a kanji. The app links a mutant to its base. |
| `components` | Its `character_component` rows: the direct components |
| `used_in` | Characters that have it as a direct component |
| `forms` | 1 `Form` for each `mutant` row that has the character as the base, for example 亻 for 人. The index `mutant_base_id` serves this lookup. Empty for a kanji with no mutants. |
| `Form.used_in` | Characters that have the mutant as a direct component, for example 休 for 亻 |
| `entries` | Full entries of the words whose written form at position 1 is exactly the character, in word id order. Empty if the character is not a word. |
| `words` | Up to `word_limit` other words whose written form at position 1 contains the character, in 3 lists. The matching form of each summary is that written form. |
| `KanjiWords.starting` | Words whose form starts with the character, for example 私立 for 私 |
| `KanjiWords.ending` | Words whose form ends with the character and does not start with it, for example 公私 |
| `KanjiWords.inside` | The other words, for example 公私混同 |
| `word_count` | Number of such words, not counting `entries` |

`components`, `used_in`, and `forms` are in code point order. `word_limit` applies to the words
of the 3 lists together, and each list keeps their order: the number of characters of the form
that are not kana, then the length of the form, then the word id. Kana do not count, so 鳥かご
(1) comes before 鳥目 (2), and 鳥打ち (2) after it. `jpdag::kana::length_without_kana`
gives the number, and `Dictionary::open` registers it as an SQL function for the query.

## 7. Errors

```rust
pub enum Error {
    Sqlite(rusqlite::Error),
    Row(serde_rusqlite::Error),
    SchemaVersion { found: i32, expected: i32 },
}
```

The error type uses `thiserror`.

## 8. Tests

The logic lives in `jpdag`, so Rust tests cover it. Each test builds a small `content.db` in a
temporary file, with `jpdag::schema::CREATE_TABLES` and a few rows, so `jpdag` does not depend on
`ingest`.

| Test | Case |
| --- | --- |
| `open_rejects_another_schema_version` | A database with another `user_version` gives `Error::SchemaVersion` |
| `empty_query_gives_no_results` | "" and "  " give no words |
| `single_kanji_query_gives_its_word_first` | 語 gives the word 語, then 語学 |
| `word_summary_points_at_its_kanji` | The word 語 has `kanji` 語, and 語学 has none |
| `exact_match_comes_first` | A word whose form equals the query comes before a longer match |
| `search_matches_readings` | A kana query finds a word by its reading |
| `search_lists_a_word_once` | A word that matches by 2 forms appears once |
| `romaji_query_finds_a_reading` | `neko` and `NEKO` find a word by its reading ねこ |
| `romaji_reads_both_spellings` | `shi` and `si` give し, `kitte` gives きって, and `ko-hi-` gives こーひー |
| `romaji_drops_an_unfinished_syllable` | `gak` gives the words of が |
| `romaji_reads_nn_as_an_input_method_does` | `onna` gives おんあ, `onnna` gives おんな, and `kannji` gives かんじ |
| `each_row_of_the_table_converts` | The letters of each kana row of the Mozc table give the kana of the row |
| `kana_do_not_count_in_the_length` | `length_without_kana` gives 1 for 鳥かご, 2 for 鳥打ち, 1 for Ｔシャツ, and 0 for コーヒー |
| `small_kana_join_the_kana_before` | `morae` gives きょ and う for きょう, が, っ, こ, and う for がっこう, and 4 morae for コーヒー |
| `letters_that_are_not_romaji_give_no_romaji_form` | `xq` searches for the typed form only |
| `hiragana_query_finds_a_katakana_reading` | こーひー finds コーヒー |
| `katakana_query_finds_a_hiragana_reading` | ネコ finds a word by its reading ねこ |
| `search_pages_do_not_overlap` | 2 pages of `limit` 2 give the same words as 1 page of `limit` 4, and a page past the end is empty |
| `search_gives_the_characters_of_the_query` | 氵 gives the character 氵 with the base 水, and no words. 食べました gives 食, with no base. 語語 gives 語 once. |
| `search_characters_do_not_depend_on_the_offset` | Each page of 語 has the character 語 |
| `summary_of_uk_word_has_no_reading` | A word whose headword is its reading has no `reading` |
| `summary_names_the_matched_form` | 鳥渡 gives ちょっと with the matched form 鳥渡, and ちょっと gives it with none |
| `headword_skips_rare_forms` | Written forms `[誤 (rK), 正]` give the headword 正 |
| `headword_of_uk_word_is_the_reading` | `uk` in the first sense gives the reading |
| `word_keeps_every_part_in_order` | Forms, readings with their morae and accents, senses, glosses, tags, and restrictions of 1 word |
| `word_hides_search_only_forms` | A written form with `sK` and a reading with `sk` are not in the entry, nor in the restrictions of a sense, and the search still finds the word by the reading |
| `written_forms_name_their_readings` | The readings of 土塊 are つちくれ and どかい, 土くれ has つちくれ alone, and a `no_kanji` reading and a reading limited to a search-only form read no form |
| `each_reading_of_a_form_has_its_furigana` | 土塊 has 1 part with つちくれ and 2 parts with ど and かい, and 土くれ has 土 with つち and くれ with none |
| `form_with_kanji_and_no_furigana_has_the_reading_as_1_part` | 逸物 with いちもつ and no `furigana` rows is 1 part with いちもつ, in the entry and in the summary |
| `form_with_no_furigana_has_no_parts` | ＡＳＣＩＩ with no `furigana` rows has no parts |
| `search_gives_the_furigana_of_the_headword` | The summary of 語学 has 語 with ご and 学 with がく, and the summary of ちょっと, whose headword is a reading, has none |
| `unknown_word_gives_none` | An id with no `word` row |
| `kanji_links_its_components_both_ways` | 休 has the components 亻 and 木, and 木 has 休 in `used_in` |
| `link_to_a_mutant_names_its_base` | In the components of 休, 亻 has the base 人, and 木 has no base |
| `mutant_has_no_page` | 亻 gives `None`, and 人 gives a `Kanji` |
| `kanji_lists_the_kanji_of_its_forms` | 人 has the form 亻 with 休 in its `used_in`, 休 is not in the `used_in` of 人, and 木 has no forms |
| `mound_and_city_are_two_forms` | 院 has the component ⻖ with the base 阜, and 部 has the component ⻏ with the base 邑. 院 is in the form ⻖ of 阜, and not in the form ⻏ of 邑. |
| `word_of_a_mutant_opens_a_word_page` | The word 攵 has no `kanji`, and its written form links 攵 with the base 攴 |
| `kanji_words_use_the_first_form` | A word that has the kanji only in position 2 is not listed |
| `kanji_word_count_exceeds_the_limit` | `word_limit` caps `words` but not `word_count` |
| `kanji_includes_its_word_entries` | The page of 私 holds its entries in full, and not in `words` |
| `kanji_has_its_readings_and_meanings` | The readings of 生 are its on readings, then its kun readings, each in order of word count. Its meanings are in position order. |
| `kanji_words_do_not_count_kana_in_their_length` | For 鳥, the order is 鳥かご, 鳥っぽい, 鳥目, then 鳥打ち |
| `kanji_words_are_split_by_the_position_of_the_kanji` | For 私, 私立 is in `starting`, 公私 is in `ending`, and 公私混同 is in `inside`. A form that starts and ends with 私 is in `starting`. |
| `character_normalizes_its_input` | ⼝ gives the page of 口 |

The `test` job of CI runs these tests.
