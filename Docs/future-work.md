# Future work

This document lists the work that the designs leave out. It is the only place for such items: a
design says what it does, and an item moves from this list into a design when its work starts.

## 1. Learning

- learning plan and FSRS reviews
- a database for the data of the user, which the app writes. A module `jpdag::user` owns it, and
  applies its schema changes with `rusqlite_migration` when it opens the file. It attaches
  `content.db` read-only, so 1 query can join both.
- bulk queries for the review planner: the kanji of every word, the words of every kanji, and the
  frequencies, without the limits that the pages use
- the school grade of each kanji (`<grade>` of KANJIDIC2), as an input of the learning plan

## 2. Frequency and order

- frequency of each word: `word.zipf`, and the priority values of `<ke_pri>` and `<re_pri>` of
  JMdict
- frequency of each character. `ingest` does not read the `<freq>` rank of KANJIDIC2.
- lists in order of frequency. These lists now end in the word id or the code point:
  - search results that tie on rules 1 to 3 of the search order
  - the words of a kanji (`Kanji.words`), which `word_limit` cuts
  - the "Used in" grids
- components in the order that KanjiVG draws them. `character_component` has no order column.

## 3. Search

- English search over glosses, with a full-text index (SQLite FTS5) that `ingest` builds
- search for a character by its reading or its meaning. `character_reading` and `character_meaning`
  have no index on `text`.

## 4. Characters

- meaning and readings of each character in the dictionary API, on each character chip, and on the
  kanji page. `content.db` has the data, from KANJIDIC2.
- the other data of a KANJIDIC2 entry: the name readings (`<nanori>`), the names of a radical
  (`<rad_name>`), the radical number, the variants, the stroke count, and the dictionary references
- characters of KANJIDIC2 that no word and no component uses
- stroke geometry of each KanjiVG entry, so the app can draw a character and show a tooltip for
  each component when the user points at its strokes

## 5. Words

- example sentences
- other text of a sense: `<lsource>`, `<xref>`, and `<ant>` of JMdict

## 6. App

- a way to replace an old copy of `content.db` when the app ships a new one. Now a developer
  deletes the copy by hand.
- an attribution page that shows the `source` table
- a touch layout. The app design assumes a window, a pointer, and a keyboard.
- accessibility: screen-reader labels of the chips, text scaling, and contrast
- widget tests: the search dropdown (the paging, the selection, and the Enter key of an input
  method), the pages, and that Flutter draws a rare character with the fallback font
