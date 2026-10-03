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

The romaji conversion, in `jpdag::kana`:

- ignores case
- reads the Hepburn and the kunrei spellings: `shi` and `si`, `tsu` and `tu`, `chi` and `ti`, `fu`
  and `hu`, `ji` and `zi`
- gives っ for a doubled consonant, ん for `nn`, `n'`, and an `n` before a consonant or at the end,
  and ー for `-`
- drops 1 consonant at the end of the query. So `gak` searches for が, and `gaku` searches for
  がく.

If any other letter is not part of a syllable, the query has no romaji form. A form that is empty
is not used.

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
}

pub struct Reading {
    pub text: String,
    pub no_kanji: bool,
    pub restricted_to: Vec<String>,
    pub tags: Vec<Tag>,
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

Each list is in `position` order. `WrittenForm.kanji` comes from `written_form_kanji`, in text
order, each as a `CharacterLink` (section 6). `Reading.restricted_to`, `Sense.written_forms`, and
`Sense.readings` hold the texts of the forms that the restrictions name. An empty list means "all
forms". An unknown id gives `None`.

## 6. Characters

```rust
pub fn character(&self, character: char, word_limit: usize) -> Result<Option<Kanji>, Error>;

pub struct Kanji {
    pub character: char,
    pub components: Vec<CharacterLink>,
    pub used_in: Vec<CharacterLink>,
    pub forms: Vec<Form>,
    pub entries: Vec<Word>,
    pub words: Vec<WordSummary>,
    pub word_count: u32,
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
| `CharacterLink.base` | Base of the character, if the character is a mutant. `None` for a kanji. The app links a mutant to its base. |
| `components` | Its `character_component` rows: the direct components |
| `used_in` | Characters that have it as a direct component |
| `forms` | 1 `Form` for each `mutant` row that has the character as the base, for example 亻 for 人. The index `mutant_base_id` serves this lookup. Empty for a kanji with no mutants. |
| `Form.used_in` | Characters that have the mutant as a direct component, for example 休 for 亻 |
| `entries` | Full entries of the words whose written form at position 1 is exactly the character, in word id order. Empty if the character is not a word. |
| `words` | Up to `word_limit` other words whose written form at position 1 contains the character |
| `word_count` | Number of such words, not counting `entries` |

`components`, `used_in`, and `forms` are in code point order. `words` are in order of form length,
then word id.

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
| `word_keeps_every_part_in_order` | Forms, readings, senses, glosses, tags, and restrictions of 1 word |
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
| `character_normalizes_its_input` | ⼝ gives the page of 口 |

The `test` job of CI runs these tests.
