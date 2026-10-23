# Sentence bundles: example sentences beside the dictionary

A sentence bundle is a SQLite database of example sentences: their text, furigana, translations,
and the JMdict words they use. The dictionary attaches bundles to `content.db`, so sentences ship
and update apart from the dictionary, and a user can add bundles of their own. The first bundle is
built from Tatoeba.

## 1. Attaching bundles

`jpdag::Dictionary::attach_bundle` attaches a bundle with SQLite's `ATTACH`, under a schema name of
its own (`bundle_1`, `bundle_2`, …). SQLite does not merge the tables of attached databases: each
bundle keeps its own `sentence`, `translation`, and `sentence_word`. So `Dictionary::sentences`
runs its query once in each bundle, and merges the results in Rust. A sentence is known by its
bundle and its id in the bundle; the words of all bundles are JMdict ids, the ids of `word` in
`content.db`.

A bundle can come from a user, so the dictionary trusts it as little as it can:

- The connection is read-only, so each bundle attaches read-only.
- `trusted_schema` is off, so the views and triggers of a file cannot call functions with side
  effects.
- A bundle must have `application_id` 0x4D545342 ("MTSB") and `user_version` 1
  (`jpdag::schema::BUNDLE_APPLICATION_ID` and `BUNDLE_SCHEMA_VERSION`), and 1 `bundle` row whose
  `id` no attached bundle has. Otherwise it detaches, and `attach_bundle` gives an error.
- Queries name each table with its schema, so a bundle cannot shadow a table of `content.db`.

SQLite attaches 10 databases at most, so a dictionary attaches 10 bundles at most.

## 2. Tables

`jpdag::schema::CREATE_BUNDLE_TABLES` holds the schema.

| Table | Content |
| --- | --- |
| `bundle` | 1 row: the id, title, version, licence, attribution, and URL of the bundle, and `sentence_url`, the page of a sentence with `{id}` for its id |
| `sentence` | The text of each sentence, its author, and its furigana |
| `translation` | The translations of each sentence, in order: language (ISO 639-3), text, author |
| `sentence_word` | The words of each sentence, keyed by word: the JMdict id, the position of the word in the sentence, the sense if known, the characters of the sentence that the word covers if known, and `checked`, whether a person marked the sentence as a good example of the word |

`sentence.furigana` is the text of the sentence with brackets, in the format of Tatoeba:
`[場所|ば|しょ]とかある？`. A bracket holds a run of kanji, then the reading of each kanji, or 1
reading for the whole run. `jpdag::dictionary::furigana_parts` reads it. The dictionary ignores
furigana that do not spell the sentence.

`sentence_word` is keyed by word, because the dictionary looks sentences up by word, and indexed
by sentence, for the words of a sentence. A word that a sentence uses twice gives its first place.

`Dictionary::sentences` gives a page of the sentences of a word: first those that a person marked
as good examples, then the shorter, across all bundles. Each bundle gives its first `offset +
limit` sentences in that order, and the page is the `limit` after the first `offset` of the merged
list, so pages do not overlap. Each sentence comes with its words: the characters
of each, and its summary, as the search gives it, for a link and its tooltip. A word that overlaps
the word before it is left out.

## 3. The Tatoeba bundle

`crates/ingest/fetch-tatoeba.sh` downloads 5 exports of Tatoeba into
`sources/tatoeba-YYYY-MM-DD/`, named by the date of the export: the Japanese sentences, their
furigana, the Japanese index, the links from Japanese to English, and the English sentences. The
binary `tatoeba` of the ingest crate builds the bundle:

```
cd crates/ingest
./fetch-tatoeba.sh
cargo run --release --bin tatoeba -- --content out/content.db \
  --tatoeba sources/tatoeba-YYYY-MM-DD --out out
```

It writes `out/tatoeba.db` and `out/tatoeba-report.txt`. Copy `tatoeba.db` to
`app/assets/bundles/`.

The bundle holds the sentences of the Japanese index of Tatoeba, which comes from the Tanaka
Corpus. Each line of the index lists the words of a sentence as `headword(reading)[sense]{form}~`:
the reading or an entry id (`#2028930`) only where the headword alone is not enough, the sense
number, the form in the sentence, and `~` for a good example. `tatoeba` finds the entry of each
word in `content.db`:

1. An entry id names its entry.
2. A kana headword names the entries with that reading and no written form, if there are any; a
   headword with kanji names the entries with that written form. A reading keeps the entries that
   have it.
3. Of several entries, the one whose first written form (or first reading, for a word in kana) is
   the headword: だ is the copula.
4. Of several still, the most frequent, by the word rank of `content.db`. A kana headword takes
   the better of its word rank and the rank of that reading in kana: の is the particle, which is
   rank 1 in kana, not 野.

A word with no entry, or with entries that the rules cannot tell apart, has no row. A sense number
past the last sense of the entry is dropped. The bundle keeps the sentences with at least 1 word
and at least 1 English translation: a sentence with no English is no example for a learner.

The build of the export of 2026-10-17 has 146,974 sentences and 1,150,184 links to words: 99.5%
of the words of the index. It leaves out 1,425 sentences with no English translation: Tatoeba has
none for them, in English or in any other language. Rule 4 decides 6.1% of the links, most of
them common particles; `tatoeba-report.txt` gives the counts. Every sentence has furigana, and a
person wrote 26% of them; Tatoeba makes the others automatically, and
they have errors, such as せい for 生 in 生の魚.

Nothing checks that the index is right: the bundle trusts its people. The index uses the senses
and entries of JMdict as they were when people indexed each sentence.

The bundle is CC BY 2.0 FR, as Tatoeba is. Each sentence keeps its author, and the app shows it.

## 4. In the app

`openBundledDictionary` copies each `.db` file of `assets/bundles/` to `bundles/` in the
application support directory on the first launch, and attaches each `.db` file of that folder in name order. A
file that fails to attach is left out, and the others load. The word page shows the sentences
under the senses, 10 at a time: a "More…" button after them loads the next 10, while there are
more. Each sentence has furigana, the word of the page in the accent colour, the first translation,
and the id and author of the sentence. Each other word of a sentence is a link to its word page: it
turns the accent colour under the pointer, and its tooltip has its headword and gloss, as the word
chips of a kanji page do. The sentence wraps between words.

The text of a sentence is the size of the written forms, and its furigana half of it. Both use
proportional widths (the OpenType feature `palt`), which take the space at the sides of kana and
punctuation away, as Japanese text sets them; kanji keep their full width.

The furigana of a sentence are set as in print, so they do not spread its text. A run of kanji
with furigana in 1 word is 1 piece, with its kana over the whole run: べんきょう over 勉強. A piece
is as wide as its text, and its kana overhang the plain text around it when they are wider. They
start with the text at the start of the sentence, and stay half a kana away from the kana of the
next piece; a piece spreads its text only when the plain text around it is too narrow, and 2 words
with furigana that touch, such as 窮鼠 and 猫, have a small space between them. The entries of a kanji page have no
sentences: each links to its word page.
