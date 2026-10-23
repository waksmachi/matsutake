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

- frequency ranks from other lists of Jiten, such as anime or novels, for words that no drama
  uses. `ingest` reads `<ke_pri>` and `<re_pri>` of JMdict only to decide between 2 words.
- the rank of a word in kana that JMdict also writes in rare kanji, with no `uk` on its first
  sense: the particle の (乃, 之) has the word rank 12,868 of 乃, though it is rank 1 in kana, so
  search puts 野 (8,032) before it. くらい (位) is the same.
- frequency of each character. `ingest` does not read the `<freq>` rank of KANJIDIC2.
- the "Used in" grids in order of frequency. They end in the code point.
- components in the order that KanjiVG draws them. `character_component` has no order column.

## 3. Search

- English search over glosses, with a full-text index (SQLite FTS5) that `ingest` builds. A query
  in ASCII can be romaji or English (`sake`, `time`), and ranking English matches well is a project
  of its own.
- search for a character by its reading or its meaning. `character_reading` and `character_meaning`
  have no index on `text`.

## 4. Characters

- meaning and readings of each character on each character chip. The kanji page shows them.
- the other data of a KANJIDIC2 entry: the name readings (`<nanori>`), the names of a radical
  (`<rad_name>`), the radical number, the variants, the stroke count, and the dictionary references
- characters of KANJIDIC2 that no word and no component uses
- stroke geometry of each KanjiVG entry, so the app can draw a character and show a tooltip for
  each component when the user points at its strokes

## 5. Words

- example sentences: the Tatoeba bundle of `Docs/bundle-design.md` is a first version. Its
  furigana are mostly automatic, and nothing checks the index. Sentences of native origin, and
  readings from a lexer of our own, would replace them.
- other text of a sense: `<lsource>`, `<xref>`, and `<ant>` of JMdict

## 6. App

- a way to replace an old copy of `content.db` when the app ships a new one. Now a developer
  deletes the copy by hand.
- the delivery of `content.db`

  **The issue.** The app ships `content.db` (97 MB) as a Flutter asset. SQLite needs a file path,
  and Flutter's asset API gives only the bytes, so on the first launch the app reads the whole
  asset into memory and writes a copy to the application support directory. The installed app
  cannot delete its asset: the bundle is read-only (and signed on iOS and macOS), and on Android
  the asset is an entry of the APK. So:

  - each device holds the data twice: about 194 MB on iOS and macOS, and about 41 MB (compressed
    in the APK) plus 97 MB on Android
  - the first launch holds the whole file in memory at once
  - the download includes the whole data, compressed by the store or the installer
  - the copy stays when the app ships a new `content.db` (the item above)

  The asset is a plain file on disk on iOS, macOS, Linux, and Windows. On Android it is not a
  file. The whole database compresses to about 29 MB with zstd and 22 MB with xz.

  **Possible solutions.** None is chosen; each answers a part of the issue.

  - Open the bundled file in place, read-only (SQLite's `immutable=1`), where it is a file: iOS,
    macOS, Linux, Windows. No copy and no stale copy, but the app works out the path of the
    asset on each platform.
  - On Android, copy with a stream: `AssetManager.open()` from a small method channel, so the
    copy needs little memory. With the file stored uncompressed in the APK (`noCompress`), the
    copy is a plain file transfer, but the APK is larger.
  - Ship the asset compressed (zstd), and decompress it in the first-launch copy: a smaller
    download and app, but always a copy.
  - The asset delivery of the stores: Play Asset Delivery on Android (a fast-follow or on-demand
    pack is a file on disk; an install-time pack stays in the APK), and On-Demand Resources on
    iOS (not on macOS). A smaller download, and no copy, but native code for each store, and the
    data depends on the store. Their size limits are to check.
  - A download at run time, for example from GitHub Releases, with a manifest of the schema
    version, size, and checksum of each file. A small app and updates of the data without an
    app release, but no data until the first download ends, and a download to make robust
    (progress, resume, checksum, an atomic swap).
  - The example sentences in a database of their own, which `content.db` attaches, so that they
    can ship and update apart from the dictionary.

- an attribution page that shows the `source` table, and the `bundle` row of each sentence
  bundle (`bundles()` gives them)
- a way to import a sentence bundle, and to replace the copy of a bundle when the app ships a new
  one. Now a user puts the file in the folder of bundles, and a developer deletes an old copy.
- a link from an example sentence to its page on Tatoeba (`Sentence.url`)
- glyphs for the characters of the bundles: `test/font_coverage_test.dart` checks `content.db`
  only. The Tatoeba bundle has 7 characters that no font of the app has, such as ♪ and ①, and the
  system draws them.
- the licence of the Mozc romaji table (`crates/jpdag/data/mozc/LICENSE`) in the licences that
  the app shows. BSD 3-Clause asks for the notice in each copy of the app.
- a touch layout. The app design assumes a window, a pointer, and a keyboard.
- accessibility: screen-reader labels of the chips, text scaling, and contrast
- widget tests: the search dropdown (the paging, the selection, and the Enter key of an input
  method), the pages, and that Flutter draws a rare character with the fallback font
