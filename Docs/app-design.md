# App: dictionary search and pages

## 1. Summary

`app/` is a Flutter frontend for the dictionary component of `jpdag`: a search screen that finds
words and characters, and a page for each word and kanji that links to its kanji and components.
The app has 1 layout for desktop and mobile: a header, then tabs, as Pleco shows an entry.

## 2. Scope

This design covers:

- `jpdag-ffi`: the bridge from Dart to `jpdag`
- `app/`: the Flutter project, its layout and pages, its font, and how it ships `content.db`

`Docs/future-work.md` lists the work that this design leaves out.

## 3. Architecture

```
app/ (Flutter, Dart)
  │  typed calls, for example dictionary.word(id: 1163940)
  ▼
crates/jpdag-ffi (flutter_rust_bridge)
  │  converts jpdag's types to Dart-friendly types
  ▼
crates/jpdag (dictionary module)
  │  SQL over rusqlite
  ▼
content.db (read-only)
```

| Part | Holds |
| --- | --- |
| `jpdag::dictionary` | API: queries and display rules, for example which written form is the headword |
| `jpdag-ffi` | 1 function for each API call, and structs that mirror the API's types. No logic. |
| `app/` | The layout, pages, navigation, and first-launch copy of `content.db` |

## 4. `jpdag-ffi`

`crates/jpdag-ffi` is a `cdylib` and `staticlib` crate with 1 module, `api`, that
`flutter_rust_bridge` reads. Its package name is `jpdag_ffi`, with no hyphen, because Cargokit makes
the file name of the library from the package name:

| Function | Calls |
| --- | --- |
| `open_dictionary(path: String) -> Result<Dictionary>` | `jpdag::Dictionary::open` |
| `search(dictionary, query, offset, limit)` | `Dictionary::search` |
| `word(dictionary, id)` | `Dictionary::word` |
| `character(dictionary, character: String, word_limit)` | `Dictionary::character` |

- `Dictionary` is an opaque type: Dart holds a handle, not the data. It wraps `jpdag::Dictionary`
  in a `Mutex`, because 1 SQLite connection serves 1 thread at a time.
- The bridge gives each `jpdag` type a Dart-friendly twin: a `char` becomes a `String` of 1
  character, and `u32` stays an integer. `From` conversions do the mapping.
- `flutter_rust_bridge` runs each call on a Rust thread pool, and Dart awaits a `Future`.
- An `Error` becomes a Dart exception.

The generated files are committed: the Rust side (`crates/jpdag-ffi/src/frb_generated.rs`) and the
Dart side (`app/lib/src/rust/`). `flutter_rust_bridge_codegen generate` rewrites them after a change
to `api`. The codegen, the Rust crate `flutter_rust_bridge`, and the Dart package
`flutter_rust_bridge` must have the same version: 2.13.0.

## 5. The Flutter app

### 5.1 Files

```
app/
  pubspec.yaml
  flutter_rust_bridge.yaml    the codegen settings: rust_root is ../crates/jpdag-ffi
  rust_builder/               the Cargokit plugin that compiles jpdag-ffi during flutter build
  assets/content.db           the output of ingest, which git ignores
  assets/bundles/             the sentence bundles that ship, such as tatoeba.db, which git ignores
  assets/fonts/               the 2 fonts and their licences (section 5.4)
  tool/subset_fallback.sh     makes the fallback font from Plangothic P1
  lib/main.dart               opens the dictionary and shows the search screen
  lib/src/dictionary.dart     the first-launch copy of content.db and the bundles, and their
                              attachment
  lib/src/search_screen.dart  the search screen, and the search session that its instances share
  lib/src/word_page.dart      the word page, and the header and tabs of each page
  lib/src/kanji_page.dart
  lib/src/character_link.dart
  lib/src/rust/               the generated bindings
  android/                    the Android runner
  ios/                        the iOS runner
  linux/                      the Linux desktop runner
  macos/                      the macOS desktop runner
```

Dart packages: `flutter_rust_bridge`, `path_provider` (the directory for the copy of `content.db`),
and the local `rust_builder` plugin. The app uses Flutter's own `Navigator` and `StatefulWidget`, not
a state or routing package.

### 5.2 Screens and pages

The app has 2 kinds of screen: **the search screen**, and **a page** for a word or a kanji. The
search screen is the first screen. A page goes on top of the screen that opens it, and the back
arrow of a page walks back through the pages, to the search screen. A page uses the full width of
the window.

**The search screen**

The search field is at the top, and the results fill the screen below it. With no query, the
screen shows a short hint: the user can type kanji, kana, or romaji.

The search screen keeps its search session while pages are open: the query, the results, and
the selected row. A page has a search icon in its top bar, which goes back to the search screen,
gives its field focus, and closes all pages. So the history of pages starts again with the next
page that the user opens.

*When the search runs*

- The search runs on each change of the text, with no delay.
- Each search has a number. When a result arrives, the app discards it if a later search has
  started.
- The search also runs on the text that an input method is still composing.

*The rows*

- **Character rows** come first: 1 row for each character of `SearchResults.characters`, with the
  character and the label "character". A character row opens the page of the character. The row of
  a mutant has the label "form of 水", and opens the page of the base.
- **Word rows** follow: the headword, the reading, and the gloss.
  - A summary with `furigana` shows the headword with its furigana, and not the reading.
  - A summary with no `reading` shows the headword alone.
  - A summary with a `matched_form` shows it after the reading, for example "matches 鳥渡".
- A word row opens the word page, also for a word that is 1 kanji, such as 私. The character row
  of the same query opens the kanji page, and the forms of the word page link to it.
- A query with characters but no words shows the character rows, then "No words". A query with
  neither shows "No results".

*More results*

- The app asks for 50 words at a time. When the user scrolls to within 10 rows of the end, or moves
  the selection there with the Down key, the app asks for the next 50 and adds them to the list.
- A page with fewer than 50 words is the last, and the app asks for no more. There is no "show
  more" button and no count.
- A new query starts again at the first page, at the top of the list.

*The keyboard*

- When results arrive for a new query, the first row is selected.
- The Up and Down keys move the selection, and Enter opens the selected row. If the results of the
  text are not yet in the list, Enter waits for them, so that it does not open a row of the query
  before.
- While an input method is composing, Enter belongs to the input method: it commits the text and
  does not open a row. The next Enter opens the selected row.
- When the search field gets focus, the app selects the whole query.

*After the user picks a row*

- The page of the row opens, and the field loses focus. The query stays in the session.

**A page**

Each page has a top bar with the back arrow and the search icon. A kanji page then has a header and
a row of tabs. The content of the selected tab fills the rest of the page, and scrolls by itself. A
tab with nothing in it is hidden: a kanji that is not a word has no Dict tab, and a kanji in no
other word has no Words tab. A page with no tab is the header alone.

**Word page**

The page has no header and no tabs: the written forms show the word. Under the top bar are the
**readings** with their written forms, a line, then the **senses**, with no section headings.

On the line of the first reading, after it and its pitch accents, is the **frequency rank** of
the word (`Word.rank`), small and in a muted colour (`outline`), after a chart icon: 675 for 綺麗.
It takes the space that the reading leaves, so it adds no line. Past `COMMON_RANK` (20,000) the
ranks are noise, so a rank shows as "uncommon" up to `RARE_RANK` (50,000), and as "rare" past it.
A word that Jiten does not rank shows "unranked". Its tooltip is headed
"Frequency rank (Jiten Dramas)", then gives the rank of each written form that has one, then of
each reading, each group in order of frequency: 奇麗 7,364, 綺麗 9,122, きれい 675, キレイ 3,490.

- The readings, in order, each on its own line. The written forms that a reading reads are on the
  next line, indented by 1 em, in brackets, with ・ between 2 forms: つちくれ, then 【土塊・土くれ】
  below it. A long list wraps, each line starts under the first form, and a line does not start
  with ・ or 】. A reading of no form, such as トリ of 鳥, has no line of forms, and no note. The
  brackets and ・ are in the colour of a divider (`outlineVariant`). The forms have no furigana.
  Each kanji of a written form is a character link.
- The pitch accent of each reading (`Reading.accents`), drawn on the reading itself: a line over
  the morae of high pitch, and a short tick down after the mora where the pitch falls. The tick
  is halfway between the ink of the 2 kana, not at the edge of their boxes, because a kana such as
  ょ has more space on 1 side of its box. `tool/kana_ink.py` reads the edges of the ink of each
  kana from Noto Sans CJK JP into `lib/src/kana_ink.dart`. A fall at the end of the reading is a
  little after the ink of the last kana. はし as 箸
  (1) has the line over は, and as 橋 (2) over し, each with a tick after it. With no fall (0), as
  端, the line runs on past the last mora with no tick, because the pitch stays high on a particle.
  So a reading with no fall looks different from a reading with no accent. The line is in the
  colour of a divider (`outlineVariant`), as the brackets and the ・ of the written forms are.
  - A reading with 2 or more accents shows the reading once for each, in order, with ・ between
    them in the colour of a divider: ちょっと (1) ・ ちょっと (0). A line does not start with ・.
  - An accent of some parts of speech has their `pos` tag names after it in small, light text,
    with ", " between them, and their descriptions as the tooltip: ちょいと adv ・ ちょいと adv, int.
  - A reading with no accent in `content.db` is plain text, with no note. Search rows and the
    tooltips of the Words tab show no accent.
- The tags of the readings and forms: a superscript number for each tag at the top right of the
  reading or form (after the first accent of a reading), such as 鳥渡¹˒², and below the last line
  of forms, a legend with the description of each number: "¹ rarely used kanji form". A number
  stands for 1 tag wherever it is, and the numbers go in the order that the tags first appear.
  The tooltip of a superscript gives its lines of the legend.
- The senses, numbered. Each sense has its tags as small, low labels (the description is the
  tooltip) on a row just above its glosses, so the glosses of all senses start in 1 column. A
  label has no fill, a thin outline in the colour of a divider (`outlineVariant`), and sharp
  corners. Then
  the glosses joined with "; " (a `lit` gloss in quotes, an `expl` gloss in italics), the note, and
  the restrictions. A part-of-speech tag shows only when it differs from the tags of the sense
  before.

**Kanji page**

- **The header**: the character, large, with its **frequency rank** (`Kanji.rank`) under it, as
  the word page shows the rank of a word: small, muted, after a chart icon, with the tooltip
  "Frequency rank (Tatoeba)". A kanji that no sentence of Tatoeba has shows "unranked". The rank
  takes the space that the readings leave beside the character, so it adds no line. Beside the
  character, the readings and the meanings:
  - A line with the label "On" and the first 3 on readings, and a line with the label "Kun" and
    the first 3 kun readings, each in the order of `Kanji.readings` (the readings of common words
    first), with a `VerticalDivider` between 2
    readings. A line with no reading is hidden.
  - A kun reading shows its okurigana in a lighter colour, without the `.`, and keeps the `-` of a
    prefix or a suffix.
  - A line with the label "Components" and the components of the kanji, each a character link,
    with a `VerticalDivider` between 2 components. A kanji with no component data has no line. The
    labels are 1 column, so the 3 lines start at the same place.
  - The first 5 meanings, with a `VerticalDivider` between 2 meanings. A long list
    wraps between meanings, and a line does not start with a divider.
- **Dict**: when the kanji is also a word, the entry of each word, 1 after the other, with a line
  between them, the most frequent word first (方 as ほう, then かた). The words with the kanji as
  a later written form follow: 路 has じ and ち, then みち (道・途・路・径). There is nothing to expand or
  collapse. An entry is short. A "Word page →" link at the right of its first reading opens the word page, which has the whole of the word: the
  frequency, the example sentences, and what the word page gains later.
  - An entry shows its readings with their written forms, as the word page does, then all its
    senses. Each kanji of a form is a character link. The kanji of the page, or a mutant of it,
    does not open the page again: under the pointer it turns a muted colour (`outline`), and the
    pointer stays an arrow.
- **Words**: the other words that contain the kanji, as chips. A chip shows the written form that
  contains the kanji. Its tooltip has the headword with its furigana (or with the reading after it,
  for a headword with no furigana), then the gloss, then in a muted colour the frequency rank of
  the word, as the word page labels it: "Frequency rank (Jiten Dramas)  18,601" for 鳥居. A tap
  opens the page of the word. The
  sub-tabs "私…", "…私", and "…私…" hold the 3 lists of `KanjiWords`, each with its count. The page
  asks for all words of the kanji, and the tab builds only the chips in view. The chips keep the
  order of `KanjiWords`, in which kana do not count in the length of a word: 鳥かご comes with the
  words of 1 kanji. Words of the same length come in order of frequency, so 鳥肌 and 鳥居 come
  before rare words. Each run of words with the same whole length is 1 grid, whose columns fit the
  words. A line in the colour of a divider separates the words of each number of kanji, and the
  number is in a narrow column at the left of its words, small and light, level with their first
  row.

**Furigana**

Furigana are the kana of each part of a headword, above that part, in the word rows of the search
and the tooltips of the Words tab. They are half the size of the headword, and 10 points at least.
A part with no kana, such as okurigana, has an empty line above it, so that all parts line up.

**Character links**

A character link is a character in the text that opens its kanji page. It looks like the text
around it, with no chip and no underline. Under the pointer, it turns vermilion and the pointer is
a hand. A mutant has no page, so its link opens the page of its base, and its tooltip says "form of
人".

The app does not show `Kanji.used_in` and `Kanji.forms`: the kanji that use a kanji.

Each link opens a new page on top of the page, so the back arrow walks back through the chain, for
example 語 → 吾 → 口.

### 5.3 Shipping `content.db`

`content.db` is an asset. SQLite cannot open a file inside the asset bundle, so on the first launch
the app copies the asset to its application support directory, and opens the copy. Later launches
open the copy.

A new app version with a new `content.db` needs a way to replace the old copy. This version does not
have one, so a developer deletes the copy by hand.

**Tooltips**

A tooltip opens up and to the right of its source: its bottom left corner is just past the top
right corner of the source (`tooltipAboveRight`). A tooltip below its source would cover the text
under it, and catch the pointer as it moves down the page. Up and to the right, the pointer can
leave the source in each direction, and reach the other sources around it. A tooltip with no room
above goes below, and a tooltip with no room at the right moves left as far as it must. The
tooltip of the search button keeps the default place, below the button, at the top of the page.

**Colours**

The theme comes from the seed #8D6E63, a brown, with 1 accent in its place: `primary` is
vermilion, #B8341F on the light background and #FF6F4F on the dark one, each with a contrast of
4.5:1 or more. The accent marks what is selected or live: the selected tab, the focused search
field, the links of the entries of a kanji page, and the word in an example sentence. A character
link turns `primary` under the pointer. Reference
content, such as the pitch accent lines, is never vermilion, so it does not look like a link.

### 5.4 Fonts

The app ships its own fonts and draws all text with them, so a kanji has its Japanese shape on
each device, and a rare component does not show as an empty box.

- **Main font: Noto Sans CJK JP** (SIL Open Font License 1.1), in the weights Regular and Bold. It
  is the default `fontFamily` of the theme. It is not Noto Sans JP, the smaller font of that name
  on Google Fonts. The app ships a subset of each weight, about 4.5 MB in place of 17 MB:
  `tool/subset_main_font.py` keeps the glyphs of each character of each text column of
  `content.db`, of printable ASCII and Latin-1, of the hiragana, katakana, CJK punctuation, and
  fullwidth forms that a query can hold, and of the strings of `lib/`. It keeps all layout
  features. Its arguments are the full fonts, from `Sans/OTF/Japanese` of
  github.com/notofonts/noto-cjk.
- **Fallback font: `fallback.ttf`**, the first entry of `fontFamilyFallback`. It holds only the
  characters of `content.db` that the main font lacks. `tool/subset_fallback.sh` makes it from
  Plangothic P1 (SIL Open Font License 1.1) with `pyftsubset` of fontTools.
- The text style of the theme has the locale `ja`. The locale of the app stays English.
- The font files and the licence of each font are in `assets/fonts/`, in the repository. The app
  adds the licences to the `LicenseRegistry` of Flutter.

When the test of section 7 finds a character with no glyph, a developer runs
`tool/subset_main_font.py` with the full fonts, and then `tool/subset_fallback.sh` with the
characters that the main font does not have. 15 characters of the glosses and forms are in no font
of the app: Latin letters with marks, such as č and ṣ, and the symbols ϕ, ≒, and ⌀. The system
draws them with a font of its own.

## 6. Build and run

```
cargo run --release -p ingest -- --jmdict crates/ingest/sources/JMdict_e.gz \
  --kanjivg crates/ingest/sources/kanjivg-20250816.xml.gz \
  --kanjidic crates/ingest/sources/kanjidic2.xml.gz \
  --furigana crates/ingest/sources/JmdictFurigana-2026-09-25.txt \
  --accents crates/ingest/sources/kanjium-accents-685d4d7.txt \
  --frequency crates/ingest/sources/jiten-drama.zip \
  --tatoeba crates/ingest/sources/tatoeba-2026-10-17 --out app/assets
cd app
fvm flutter run -d linux    # or: -d macos, or an iOS or Android device
```

`fvm` runs the Flutter version that `.fvmrc` pins. `flutter run` compiles `jpdag-ffi` through
Cargokit. Cargokit builds with the `stable` toolchain of rustup, and installs it if it is missing.
The macOS and iOS builds also need Xcode and CocoaPods, and the iOS build needs the iOS platform of
Xcode. The Android build needs the Android SDK and NDK. `ingest` also writes `build-report.txt` into
`app/assets`, so `pubspec.yaml` lists `assets/content.db` alone, and `.gitignore` ignores the
report.

To type Japanese into the Linux build under WSL, the WSL distribution needs an input method, for
example fcitx5 with Mozc. Pasting works without one.

## 7. Tests and CI

The logic lives in `jpdag`, so its Rust tests cover it (`Docs/jpdag-dictionary-design.md`,
section 8). The behaviour of the search dropdown (the paging, the selection, and the Enter key of an
input method) has no test in this version.

**Each character has a glyph.** `app/test/font_coverage_test.dart` checks the fonts of the app
against the `content.db` in `app/assets/`:

| Test | Condition |
| --- | --- |
| `each character has a glyph` | For each `character` row, and each character of each text column, the character map of Noto Sans CJK JP Regular or of `fallback.ttf` holds the code point, unless it is 1 of the 15 characters that the system draws. The Bold weight holds each code point that the Regular weight holds. The message of a failure lists each character with no glyph, with its code point. |
| `fallback font has no unused glyph` | Each code point of `fallback.ttf` is a character of `content.db` that Noto Sans CJK JP lacks |

The test reads `content.db` with the Dart package `sqlite3`, a development dependency, and
reads the character map (the `cmap` table) of each font file with a small reader in
`app/test/support/`. If `app/assets/content.db` does not exist, the test is skipped with a message.
The test compares code points, and does not draw text.

### CI

CI gets a third job, `app`. It runs after the `build-checks` job of `ingest`, and uses the
`content.db` of that job:

1. Download the `content-db` artifact of `build-checks` into `app/assets/`.
2. Install the pinned Flutter with fvm.
3. Run `flutter_rust_bridge_codegen generate`, and report an error if the generated files change.
4. Run `flutter analyze`.
5. Run `flutter test`.

The workflow also runs each Monday on the new data of the sources.
