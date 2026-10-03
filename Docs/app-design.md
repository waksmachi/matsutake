# App: dictionary search and pages

## 1. Summary

`app/` is a Flutter frontend for the dictionary component of `jpdag`: a search bar that finds words
and characters, and a page for each word and kanji that links to its kanji and components. A
mutant, for example 亻, has no page: it links to its base kanji 人, and the page of 人 lists the
kanji that use 亻. A kanji that is also a word has 1 page for both.

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
| `jpdag::dictionary` | API: every query and every display rule, for example which written form is the headword |
| `jpdag-ffi` | 1 function for each API call, and structs that mirror the API's types. No logic. |
| `app/` | The layout, pages, navigation, and first-launch copy of `content.db` |

## 4. `jpdag-ffi`

`crates/jpdag-ffi` is a `cdylib` and `staticlib` crate with 1 module, `api`, that
`flutter_rust_bridge` reads:

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
  assets/fonts/               the 2 fonts and their licences (section 5.4)
  tool/subset_fallback.sh     makes the fallback font from Plangothic P1
  lib/main.dart               opens the dictionary and shows the layout
  lib/src/dictionary.dart     the first-launch copy of content.db
  lib/src/layout.dart         the search bar, the dropdown, and the content area with its history
  lib/src/search_dropdown.dart
  lib/src/word_page.dart
  lib/src/kanji_page.dart
  lib/src/character_chip.dart
  lib/src/rust/               the generated bindings
  linux/                      the Linux desktop runner
```

Dart packages: `flutter_rust_bridge`, `path_provider` (the directory for the copy of `content.db`),
and the local `rust_builder` plugin. The app uses Flutter's own `Navigator` and `StatefulWidget`, not
a state or routing package.

### 5.2 Layout and pages

**The layout** has 2 parts:

- **The search bar**, fixed at the top. A back button at its left goes back in the history of the
  content area. The button is hidden when there is nothing to go back to.
- **The content area** below it. It shows 1 page at a time: a start page with a short hint (the
  user can type kanji, kana, or romaji), a word page, or a kanji page. It has its own
  nested `Navigator`: each link pushes a page into that history, and the search bar stays in
  place.

**The search dropdown**

The dropdown opens under the search bar while the field has focus and the query is not empty. It
floats over the content area, which stays visible around it. The dropdown scrolls, and its height is
at most 60% of the window.

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
  - A summary with no `reading` shows the headword alone.
  - A summary with a `matched_form` shows it after the reading, for example "matches 鳥渡".
- A word whose `WordSummary.kanji` is set, for example 私, opens the page of that character with
  the word's entry expanded (see the kanji page), not a word page.
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
- The Up and Down keys move the selection, Enter opens the selected row, and Escape closes the
  dropdown. A click outside the dropdown also closes it.
- While an input method is composing, Enter belongs to the input method: it commits the text and
  does not open a row. The next Enter opens the selected row.

*After the user picks a row*

- The row opens in the content area, the dropdown closes, and the field loses focus. The query
  stays in the field.
- When the field gets focus again, the app selects the whole query, and the dropdown opens again
  with the same results at the same scroll position.

**Word page**

- The headword is the title, with the reading below it.
- **Written forms**: each form with its tags. Each kanji of the form is a character chip.
- **Readings**: each reading with its tags, its restrictions ("only 一寸"), and a mark when it has no
  written form.
- **Senses**, numbered: the tags as small chips (the description is the tooltip), the glosses joined
  with "; " (a `lit` gloss in quotes, an `expl` gloss in italics), the note, and the restrictions.

**Kanji page**

- **The header**: the character, large. Beside it, **Components**: character chips. None for a
  character with no component data.
- **Entries**: when the kanji is also a word, 1 row for each of its word entries.
  - A collapsed row shows the reading at position 1 and the glosses of the first sense.
  - A tap expands the row to the full entry, as on the word page, and a second tap collapses it.
  - A kanji with 1 entry shows it expanded.
  - When the page opens from a word row of the search, that entry is expanded and has a highlight,
    and the page scrolls only as far as it must to show the entry. When the page opens from a
    character row or a character link, the page opens at the top.
- **Used in**: a grid of character chips. After it, 1 grid for each form of the kanji, with the
  heading "as 氵": the kanji that use that mutant. A grid with no characters is hidden.
- **Words**: the other words that contain the kanji, with "N more" when `word_count` is larger than
  the list.

**Character chips**

Each linked character on a page is a chip: the character alone, with no meaning and no reading. A
tap opens the kanji page of the character.

A mutant has no page, so its chip is different in 2 ways:

- **Style**: a dashed outline, and the tooltip "form of 人".
- **Target**: a tap opens the page of the base.

Each link pushes a new page into the content area, so the back button walks back through the chain,
for example 語 → 吾 → 口.

### 5.3 Shipping `content.db`

`content.db` is an asset. SQLite cannot open a file inside the asset bundle, so on the first launch
the app copies the asset to its application support directory, and opens the copy. Later launches
open the copy.

A new app version with a new `content.db` needs a way to replace the old copy. This version does not
have one, so a developer deletes the copy by hand.

### 5.4 Fonts

The app ships its own fonts and draws all text with them, so a kanji has its Japanese shape on
each device, and a rare component does not show as an empty box.

- **Main font: Noto Sans CJK JP** (SIL Open Font License 1.1), in the weights Regular and Bold. It
  is the default `fontFamily` of the theme. It is not Noto Sans JP, the smaller font of that name
  on Google Fonts.
- **Fallback font: `fallback.ttf`**, the first entry of `fontFamilyFallback`. It holds only the
  characters of `content.db` that the main font lacks. `tool/subset_fallback.sh` makes it from
  Plangothic P1 (SIL Open Font License 1.1) with `pyftsubset` of fontTools.
- The text style of the theme has the locale `ja`. The locale of the app stays English.
- The font files and the licence of each font are in `assets/fonts/`, in the repository. The app
  adds the licences to the `LicenseRegistry` of Flutter.

When the test of section 7 finds a character with no glyph, a developer runs
`tool/subset_fallback.sh` with the characters that the test names.

## 6. Build and run

```
cargo run --release -p ingest -- --jmdict crates/ingest/sources/JMdict_e.gz \
  --kanjivg crates/ingest/sources/kanjivg-20250816.xml.gz \
  --kanjidic crates/ingest/sources/kanjidic2.xml.gz --out app/assets
cd app
fvm flutter run -d linux
```

`fvm` runs the Flutter version that `.fvmrc` pins. `flutter run` compiles `jpdag-ffi` through
Cargokit. `ingest` also writes `build-report.txt` into `app/assets`, so `pubspec.yaml` lists
`assets/content.db` alone, and `.gitignore` ignores the report.

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
| `each character has a glyph` | For each `character` row, the character map of Noto Sans CJK JP Regular or of `fallback.ttf` holds the code point. The Bold weight holds each code point that the Regular weight holds. The message of a failure lists each character with no glyph, with its code point. |
| `fallback font has no unused glyph` | Each code point of `fallback.ttf` is a `character` row that Noto Sans CJK JP lacks |

The test reads the `character` table with the Dart package `sqlite3`, a development dependency, and
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
