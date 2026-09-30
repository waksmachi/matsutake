# Ingest: JMdict and components

This document describes the design of the `ingest` crate. `ingest` populates `content.db`, the
database of the app, from free public data sources.

## 1. Summary

`content.db` holds a graph of words and characters. The graph has 3 kinds of link:

- A written form of a word links to its kanji. 語学 links to 語 and 学.
- A character links to its direct components. 語 links to 言 and 吾. 吾 links to 五 and 口.
- A mutant links to its base. 亻 is the form of 人 on the left side of a kanji, so 亻 links
  to 人.

The `ingest` crate builds the graph in stages.

1. *JMdict* gives the words, with their written forms, readings, senses, glosses, and tags.
2. *KanjiVG* gives the component tree of each character.
3. `data/mutants.tsv`, a list in this repository, gives the mutant mapping.
4. Graph is written from memory into `content.db`.

## 2. Scope

This design covers 17 tables in 4 groups.

The graph:

| Table | Content |
| --- | --- |
| `word` | 1 row for each JMdict entry |
| `written_form_kanji` | Kanji of each written form |
| `character` | Each character that the other tables refer to |
| `character_component` | Direct components of each character |
| `mutant` | Base of each mutant |

The structure of each JMdict entry:

| Table | Content |
| --- | --- |
| `written_form` | 1 row for each `<k_ele>` |
| `reading` | 1 row for each `<r_ele>` |
| `reading_restriction` | Written forms that a reading applies to (`<re_restr>`) |
| `sense` | 1 row for each `<sense>`, with its note (`<s_inf>`) |
| `gloss` | English glosses of each sense (`<gloss>`) |
| `sense_written_form` | Written forms that a sense applies to (`<stagk>`) |
| `sense_reading` | Readings that a sense applies to (`<stagr>`) |

The JMdict tags:

| Table | Content |
| --- | --- |
| `tag` | 1 row for each tag, with its category and its description |
| `written_form_tag` | `<ke_inf>` tags of each written form |
| `reading_tag` | `<re_inf>` tags of each reading |
| `sense_tag` | `<pos>`, `<field>`, `<misc>`, and `<dial>` tags of each sense |

The provenance of the data:

| Table | Content |
| --- | --- |
| `source` | 1 row for each source, with its version, its licence, and its attribution text |

Later designs add these items:

- example sentences
- general frequency (`word.zipf`), and the priority values of `<ke_pri>` and `<re_pri>`
- other text of a sense: `<lsource>`, `<xref>`, and `<ant>`
- stroke geometry of each KanjiVG entry, so the app can draw a character and show a tooltip
  for each component when the user points at its strokes

## 3. Terms

| Term | Meaning |
| --- | --- |
| Character | 1 code point after normalization. `character.id` is the code point. |
| Normalization | Function `jpdag::normalize`, which the pipeline applies to each code point from a source (section 6.1) |
| Written form | A spelling of a JMdict entry that is not kana alone. Each `<keb>` element gives 1 written form. Most written forms contain kanji, but some do not, for example ＵＦＯ and ビタミンＢ６. |
| Reading | A written form of a JMdict entry in kana. Each `<reb>` element gives 1 reading. |
| Sense | 1 meaning of a JMdict entry. Each `<sense>` element gives 1 sense. |
| Gloss | An English word or phrase for 1 sense, for example "a little". Each `<gloss>` element gives 1 gloss. A sense has 0 or more glosses. |
| Sense note | A free-text note about the use of 1 sense, for example "before a verb in negative form". The `<s_inf>` element gives the note. |
| Tag | A JMdict entity, for example `uk`, in 1 of 6 categories: `ke_inf`, `re_inf`, `pos`, `field`, `misc`, or `dial` |
| Group | A `<g>` element of KanjiVG. A group holds the strokes of 1 part of a character. |
| KanjiVG entry | A `<kanji>` element of KanjiVG that stage 3 keeps. A KanjiVG entry describes 1 character. |
| Direct component | A component at the highest level of the groups of a character (section 6.4). 言 and 吾 are the direct components of 語. |
| KanjiVG component | A character that is a direct component of 1 or more KanjiVG entries |
| Mutant | A variant of a kanji that occurs as a component, for example 氵 |
| Base | Kanji of a mutant, for example 水 for 氵 |
| Uncovered character | A character with no KanjiVG entry, because KanjiVG does not cover it. An uncovered character has no `character_component` rows. |

## 4. Datasources

| Source | File | Publisher | Licence | Use |
| --- | --- | --- | --- | --- |
| JMdict | `JMdict_e.gz` | EDRDG | CC BY-SA 4.0 | Entries, written forms, readings, senses, and tags |
| KanjiVG | `kanjivg-YYYYMMDD.xml.gz`, from the GitHub release | KanjiVG project | CC BY-SA 3.0 | Components of each character |

The `fetch.sh` script downloads the sources into `crates/ingest/sources/`. The script sets the
KanjiVG release, for example r20250816. JMdict has no releases, so the script downloads the current
file. EDRDG publishes a new JMdict file each day.

`data/mutants.tsv` is not a download. The first 19 rows of the file come from `japanese-radicals.csv`
of Kanji alive (CC BY 4.0), and the file's header credits Kanji alive (section 6.5).

`content.db` is a derivative of JMdict and KanjiVG, so CC BY-SA applies to it. The `source` table
holds the attribution that each licence asks for, so the app can show it.

The `ingest` binary reads local files only and has no network code. The arguments of the binary give
the path of each source file and the output directory. So the tests run offline, and a build uses
exactly the files that the arguments give.

## 5. Output schema

`jpdag::schema` holds these statements and `SCHEMA_VERSION`, the value of `PRAGMA user_version`.

```sql
CREATE TABLE word (
  id INTEGER PRIMARY KEY                                    -- JMdict ent_seq
) STRICT;

CREATE TABLE character (
  id INTEGER PRIMARY KEY                                    -- code point after normalization
) STRICT;

CREATE TABLE character_component (
  character_id INTEGER NOT NULL REFERENCES character(id),
  component_id INTEGER NOT NULL REFERENCES character(id),
  PRIMARY KEY (character_id, component_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE written_form_kanji (
  written_form_id INTEGER NOT NULL REFERENCES written_form(id),
  kanji_id      INTEGER NOT NULL REFERENCES character(id),
  PRIMARY KEY (written_form_id, kanji_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE mutant (
  mutant_id INTEGER NOT NULL REFERENCES character(id),
  base_id   INTEGER NOT NULL REFERENCES character(id),
  PRIMARY KEY (mutant_id, base_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE written_form (
  id       INTEGER PRIMARY KEY,
  word_id  INTEGER NOT NULL REFERENCES word(id),
  position INTEGER NOT NULL,
  text     TEXT NOT NULL,
  UNIQUE (word_id, position)
) STRICT;

CREATE TABLE reading (
  id       INTEGER PRIMARY KEY,
  word_id  INTEGER NOT NULL REFERENCES word(id),
  position INTEGER NOT NULL,
  text     TEXT NOT NULL,
  no_kanji INTEGER NOT NULL CHECK (no_kanji IN (0, 1)),
  UNIQUE (word_id, position)
) STRICT;

CREATE TABLE reading_restriction (
  reading_id    INTEGER NOT NULL REFERENCES reading(id),
  written_form_id INTEGER NOT NULL REFERENCES written_form(id),
  PRIMARY KEY (reading_id, written_form_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE sense (
  id       INTEGER PRIMARY KEY,
  word_id  INTEGER NOT NULL REFERENCES word(id),
  position INTEGER NOT NULL,
  note     TEXT,
  UNIQUE (word_id, position)
) STRICT;

CREATE TABLE gloss (
  sense_id INTEGER NOT NULL REFERENCES sense(id),
  position INTEGER NOT NULL,
  text     TEXT NOT NULL,
  type     TEXT,
  PRIMARY KEY (sense_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE sense_written_form (
  sense_id      INTEGER NOT NULL REFERENCES sense(id),
  written_form_id INTEGER NOT NULL REFERENCES written_form(id),
  PRIMARY KEY (sense_id, written_form_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE sense_reading (
  sense_id   INTEGER NOT NULL REFERENCES sense(id),
  reading_id INTEGER NOT NULL REFERENCES reading(id),
  PRIMARY KEY (sense_id, reading_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE tag (
  id          INTEGER PRIMARY KEY,
  category    TEXT NOT NULL,
  name        TEXT NOT NULL,
  description TEXT,
  UNIQUE (category, name)
) STRICT;

CREATE TABLE written_form_tag (
  written_form_id INTEGER NOT NULL REFERENCES written_form(id),
  tag_id        INTEGER NOT NULL REFERENCES tag(id),
  PRIMARY KEY (written_form_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE reading_tag (
  reading_id INTEGER NOT NULL REFERENCES reading(id),
  tag_id     INTEGER NOT NULL REFERENCES tag(id),
  PRIMARY KEY (reading_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE sense_tag (
  sense_id INTEGER NOT NULL REFERENCES sense(id),
  tag_id   INTEGER NOT NULL REFERENCES tag(id),
  PRIMARY KEY (sense_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE source (
  name        TEXT PRIMARY KEY,                             -- JMdict, KanjiVG, or Kanji alive
  version     TEXT,                                         -- the file's date
  licence     TEXT NOT NULL,
  attribution TEXT NOT NULL
) STRICT;

CREATE INDEX written_form_text                  ON written_form(text);
CREATE INDEX reading_text                     ON reading(text);
CREATE INDEX written_form_kanji_kanji_id        ON written_form_kanji(kanji_id);
CREATE INDEX character_component_component_id ON character_component(component_id);
CREATE INDEX mutant_base_id                   ON mutant(base_id);
CREATE INDEX written_form_tag_tag_id            ON written_form_tag(tag_id);
CREATE INDEX reading_tag_tag_id               ON reading_tag(tag_id);
CREATE INDEX sense_tag_tag_id                 ON sense_tag(tag_id);
```

The primary key of a table serves lookups in the direction of its first column. The indexes serve
the other direction: a word by its written form, the words that use a kanji, the characters that
contain a component, the mutants of a base, and the written forms, readings, or senses with a tag.

`word` is represented by its JMdict `ent_seq`, and `character` is its code point.

Stage 2 gives the ids of `written_form`, `reading`, `sense`, and `tag` in the order of the word id and
the position, so 2 builds from the same files give the same ids. The ids change when JMdict changes.
For a reference that lasts across builds, the app must store the word id and the position, not the
id.

### Character identity

Normalization merges code points that look identical. A mutant and its base look different,
so they stay 2 characters, and `mutant` links them.

| Code points | merged? |
| --- | --- |
| 亻 (U+4EBB) and 人 (U+4EBA) | No |
| ⻞ (U+2EDE) and 飠 (U+98E0) | No |
| ⻖ (U+2ED6), ⻏ (U+2ECF), and 阝 (U+961D) | Yes |
| ⺨ (U+2EA8) and 犭 (U+72AD) | Yes |

## 6. Pipeline

```
  JMdict_e.gz            kanjivg-*.xml.gz        data/mutants.tsv
       │                        │                       │
       ▼                        ▼                       ▼
  1 Parse JMdict          3 Parse KanjiVG ─────► 4 Read the mutants
       │                        │                       │
       ▼                        │                       │
  2 Build the word rows         │                       │
       │    │                   ▼                       │
       │    └─────────► 5 Complete the  ◄───────────────┘
       │                  character set
       │                        │
       │                        ▼
       └──────────────► 6 Write content.db
```

| Stage | Input | Output |
| --- | --- | --- |
| 1. Parse JMdict | `JMdict_e.gz` | Each entry, with its written forms, readings, senses, and tags |
| 2. Build the word rows | Stage 1 | Rows of `word`, `written_form_kanji`, the entry tables, and the tag tables |
| 3. Parse KanjiVG | `kanjivg-*.xml.gz` | Direct components of each KanjiVG entry |
| 4. Read the mutants | `data/mutants.tsv`, and the KanjiVG components of stage 3 | Candidate `mutant` rows |
| 5. Complete the character set | Stages 2, 3, and 4 | `character`, `character_component`, and `mutant` rows |
| 6. Write `content.db` | Stages 1, 2, 3, and 5 | `content.db` |

### 6.1 Normalization

Sources can write a particular character with different code points. For example, a source can write
the mouth radical as ⼝ (U+2F1D) or as 口 (U+53E3). Without normalization, these code points give 2
`character` rows for 1 character.

`jpdag::normalize` normalizes 1 code point, and gives 1 or more code points:

1. Apply [Normalization Form KC](https://www.unicode.org/reports/tr15/) (NFKC) to the code point.
2. If the From column of the table below holds the result, replace the result with the To value.

| From | To |
| --- | --- |
| ⻖ (U+2ED6) | 阝 (U+961D) |
| ⻏ (U+2ECF) | 阝 (U+961D) |
| ⺨ (U+2EA8) | 犭 (U+72AD) |
| ⺉ (U+2E89) | 刂 (U+5202) |
| ⺡ (U+2EA1) | 氵 (U+6C35) |

NFKC merges the Kangxi radicals and most compatibility ideographs with their unified ideographs. NFKC
does not change most code points of the CJK Radicals Supplement block. The table merges the 5 code
points of that block that look identical to a unified ideograph. Each To value is a unified
ideograph.

The result can have more than 1 code point. For example, normalization gives 平成 for ㍻ (U+337B).

### 6.2 Stage 1: Parse JMdict

1. Read `JMdict_e.gz` as a stream of XML events.
2. Read each entity declaration of the DTD, and keep its name and its text.
3. For each `<entry>`, keep the `<ent_seq>` value.
4. For each `<k_ele>`, keep the `<keb>` text and the `<ke_inf>` tags.
5. For each `<r_ele>`, keep the `<reb>` text, the `<re_inf>` tags, and the `<re_restr>` texts. Keep
   whether the element holds `<re_nokanji/>`.
6. For each `<sense>`, keep the `<stagk>` and `<stagr>` texts. Keep the `<pos>`,
   `<field>`, `<misc>`, and `<dial>` tags. Keep the text of each `<gloss>`, and its `g_type` value.
   Keep the text of each `<s_inf>`.
7. Discard all other elements.

Stage 1 keeps the written forms, readings, senses, and glosses of an entry in file order.
Later designs read the elements that step 7 discards, for example `<lsource>`.

JMdict writes each tag as an entity reference, for example `&uk;`. The document type definition (DTD)
gives a text for each entity, for example "word usually written using kana alone". The parser keeps
the name of the entity as the tag, and keeps the text as the description of the tag. So `&uk;`
becomes the tag `uk`, with the description "word usually written using kana alone". An entity that
the DTD does not declare also keeps its name, and its tag has no description.

Stage 1 also reads the file's version: the date in the comment `<!-- JMdict created:
YYYY-MM-DD -->`. If the file has no such comment, the version is NULL, and the build report says
so.

A parse error gives the line number in the decompressed file.

### 6.3 Stage 2: Build the word rows

Stage 2 turns each entry of stage 1 into rows. Each JMdict entry gives 1 `word` row.

#### Entry rows

1. For each written form of the entry, add 1 `written_form` row with its position.
2. For each reading of the entry, add 1 `reading` row with its position.
3. For each sense of the entry, add 1 `sense` row with its position and its note.
4. For each gloss of a sense, add 1 `gloss` row with its position in the sense and its `g_type`
   value.
5. For each `<re_restr>` text, add 1 `reading_restriction` row. The row links the reading to the
   written form of the same entry with that text.
6. For each `<stagk>` text, add 1 `sense_written_form` row in the same way.
7. For each `<stagr>` text, add 1 `sense_reading` row in the same way.
8. If a `<re_restr>`, `<stagk>`, or `<stagr>` text names no form of its entry, skip the restriction.
9. If a sense has no `<pos>` tag, give the sense the `<pos>` tags of the previous sense.
10. For each tag, add 1 `written_form_tag`, `reading_tag`, or `sense_tag` row.

The `g_type` value marks a gloss that is not a plain translation: `lit` (literal), `fig`
(figurative), `expl` (an explanation), or `tm` (a trademark). Most glosses have no `g_type`, so
`gloss.type` is NULL. The DTD lets a sense have no gloss, for a sense that is only a cross-reference.

`sense.note` is the text of the `<s_inf>` of the sense, or NULL if the sense has no `<s_inf>`. The
DTD lets a sense have more than 1 `<s_inf>`. In that case, `sense.note` joins the texts with "; ".

A restriction that names no form is an error in the source. Step 8 skips it, and the build report
lists it, so 1 error in a daily JMdict file does not stop the build. The reading or sense of a
skipped restriction then has no restriction for that form.

Step 9 applies a rule of the DTD: the `<pos>` tags of an earlier sense apply to the later senses,
until a sense gives new `<pos>` tags.

The category of a tag is the element that holds the tag. Each pair of category and name gives 1 `tag`
row, and the DTD gives its description. The name `ik` occurs in 2 elements, so `ik` gives 2 `tag`
rows.

#### Kanji links

The `written_form_kanji` rows link each written form to its kanji. For each written form, do these steps:

1. Normalize each code point of the written form.
2. Keep each code point with the Unicode property `Unified_Ideograph`.
3. Remove duplicate code points.
4. Add 1 `written_form_kanji` row for each code point in the result.

A written form without kanji, for example ＵＦＯ, gets no `written_form_kanji` rows. Stage 2 links
every other written form, including a form with the tag `iK`, `oK`, `rK`, or `sK`, and the
forms of a word whose senses have the tag `uk`. So ちょっと links to 鳥 and 渡 through its rare form 鳥渡. A
query that needs only the usual written forms filters the written forms. For example, the query can
take only the written form at position 1, exclude the forms with those 4 tags in `written_form_tag`, or
exclude the words whose first sense has `uk` in `sense_tag`. JMdict almost always lists the usual kanji
form first.

Step 2 removes kana, Latin letters, and marks. 々, 〆, and ヶ have no `Unified_Ideograph` property, so
人々 gives only 人, and 〆切 gives only 切. The ranges of the property come from
`PropList-18.0.0.txt`.

| Written form | `written_form_kanji` rows |
| --- | --- |
| 取り扱い | 取, 扱 |
| 一ヶ月 | 一, 月 |
| Tシャツ | None |
| 𠮟る (U+20B9F) | U+20B9F |
| U+FA19, a compatibility ideograph | 神 (U+795E), after normalization |
| 﨑 (U+FA11), a unified ideograph in the compatibility block | U+FA11 |

### 6.4 Stage 3: Parse KanjiVG

KanjiVG draws each character as a tree of groups. The top group is the `<g>` element directly inside
a `<kanji>` element. Each group can name its part in a `kvg:element` attribute. This tree shows the
groups of 語:

```
語                    the top group
├── 言                a direct component of 語
│   └── 口            not visited
└── 吾                a direct component of 語
    ├── 五            not visited
    └── 口            not visited
```

Stage 3 walks down from the top group. On each path, the first group that names 1 character, other
than the character of the entry, gives a direct component. The walk stops there. The direct
components of 吾 come from the KanjiVG entry of 吾, not from the groups inside 語. So each character
has 1 set of direct components.

1. Read `kanjivg-YYYYMMDD.xml.gz` as a stream of XML events.
2. Take each `<kanji>` element whose `id` ends with the code point, for example `kvg:kanji_08a9e`.
3. Ignore each `<kanji>` element whose `id` has a suffix after the code point, for example
   `kvg:kanji_08a9e-Kaisho`.
4. Read the code point from the hexadecimal digits of the `id`.
5. If normalization changes the code point, ignore the `<kanji>` element.
6. Visit the child groups of the top group.
7. If a group has a `kvg:element` attribute, normalize the value.
8. If the result is 1 character, and not the character of the entry, record the result as a direct
   component.
9. If a group gives a direct component, do not visit the child groups of that group.
10. If a group gives no direct component, visit the child groups of that group.
11. Remove duplicate direct components.

Step 3 ignores the variant styles of a character, such as Kaisho, because the standard entry gives
the components. Step 5 ignores the entries of code points that `character` never holds. The KanjiVG
entry of the normalized code point gives the components of that character.

The parser reads `kvg:element` and ignores `kvg:original`. `kvg:element` names the shape as drawn,
and `kvg:original` names the kanji that the shape comes from. A group with `kvg:element="亻"` and
`kvg:original="人"` gives the direct component 亻. The link from 亻 to 人 comes from
`mutant` (stage 4).

KanjiVG gives some components without a code point. KanjiVG then uses a Chinese Document Processing
(CDP) code, for example `CDP-8BD0`, or an ideographic description sequence, for example `⿱日隹`. A
group with such a component gives no direct component, so the parser visits its child groups. The
build report lists these groups.

Stage 3 also reads the file's version: the date in the header comment "This file was generated
on YYYY-MM-DD". If the file has no such comment, the version is NULL, and the build report says so.

### 6.5 Stage 4: Read the mutants

`data/mutants.tsv` lists the mutants and their bases. A person keeps the list. Each line has 2
columns, separated by a tab: the mutant and base. A line that starts with `#` is a comment. The
file is `crates/ingest/data/mutants.tsv`. The binary includes the file when it compiles, so the file
is not an argument of the binary, and the integration test uses the same list.

1. Read `data/mutants.tsv`.
2. Normalize the mutant and base of each line.
3. If a column does not give exactly 1 character, stop the build.
4. If the mutant and base are the same character, stop the build.
5. If 2 lines give the same mutant and base, stop the build.
6. If a mutant is not a KanjiVG component, list the mutant in the build report.
7. Add each line as a stage 4 row.

Steps 3 to 5 stop the build, because an error in the list is an error in this repository, not in a
source. A mutant can have more than 1 base. After normalization, ⻖ (the left form of 阜) and ⻏ (the
right form of 邑) are both 阝, so 阝 has 2 rows.

The first 19 rows come from `japanese-radicals.csv` of Kanji alive:

| Mutant | Base | Mutant | Base | Mutant | Base |
| --- | --- | --- | --- | --- | --- |
| 亻 | 人 | 氵 | 水 | 扌 | 手 |
| 忄 | 心 | ⺗ | 心 | ⺌ | 小 |
| 飠 | 食 | 刂 | 刀 | 犭 | 犬 |
| 灬 | 火 | ⺤ | 爪 | 耂 | 老 |
| 礻 | 示 | 衤 | 衣 | ⻌ | 辵 |
| 攵 | 攴 | 艹 | 艸 | 阝 | 阜 |
| 阝 | 邑 | | | | |

The other 4 rows are additions:

| Mutant | Base | Reason |
| --- | --- | --- |
| ⻞ | 食 | Kanji alive gives 飠 as a form of 食, but not ⻞. KanjiVG uses both. |
| 罒 | 网 | Kanji alive writes the net top as ⺫ (U+2EAB), the code point of the eye radical. |
| 氺 | 水 | Kanji alive gives 氺 with no position. |
| 覀 | 襾 | Kanji alive gives the west top as ⻃ with no position. |

Some bases occur in few words. For example, 艸 occurs in 4 JMdict entries, and 辵 occurs in 1. The
list keeps these rows, because `mutant` records the base that each mutant comes from.

### 6.6 Stage 5: Complete the character set

Stage 5 finds each character that `content.db` needs. The set starts with the kanji of the words.
Then the set grows with the components of each character and the base of each mutant, until
no new character joins. Each character in the set gives 1 `character` row. Each direct component of a
character gives 1 `character_component` row.

1. Add each kanji of the stage 2 rows to the character set.
2. Take a character from the set that stage 5 did not examine.
3. If the character has a KanjiVG entry, add each direct component of the character to the set.
4. If the character is the mutant of a stage 4 row, add the base of the row to the set.
5. Repeat steps 2 to 4 until stage 5 examines each character in the set.
6. Discard each stage 4 row whose mutant is not in the set.

Step 4 adds 人 to the set when 亻 is in the set, also if no word uses 人. Step 6 keeps only the
mutants that the graph uses.

An uncovered character has a `character` row and no `character_component` rows. For example, 𠮟 is
the kanji of a word, and 𠂉 is a component of 毎. KanjiVG has no entry for either character, so both
are uncovered characters.

### 6.7 Stage 6: Write `content.db`

Stage 6 writes a new file and replaces `content.db` only after all checks pass. A failed build thus
leaves the previous `content.db` unchanged.

1. Open `content.db.tmp` in the output directory.
2. Create the 17 tables and the indexes with the statements of `jpdag::schema`.
3. Set `PRAGMA user_version` to `SCHEMA_VERSION`.
4. Insert all rows in 1 transaction.
5. Run `PRAGMA foreign_key_check`.
6. If the check returns a row, stop the build.
7. Run `VACUUM`.
8. Rename `content.db.tmp` to `content.db`.

The foreign keys are off during the insert. Step 5 checks all rows at the end, and the error gives
the number of rows that fail. A failed build deletes `content.db.tmp`.

Stage 6 writes 1 `source` row for JMdict, KanjiVG, and Kanji alive. The code holds the licence and
the attribution text of each source. The versions of JMdict and KanjiVG come from stages 1 and 3.
The version of Kanji alive is NULL, because `data/mutants.tsv` holds its data.

The `rusqlite` crate compiles its own SQLite, so `ingest` and the app use the same SQLite version.

## 7. Build report

The binary prints the build report and writes it to `build-report.txt` in the output directory. The
report shows what the sources gave and what the pipeline discarded:

- version of each source
- number of rows in each table
- number of characters with a KanjiVG entry, and the number of uncovered characters
- each kanji of a word with no KanjiVG entry
- each direct component with no KanjiVG entry
- each JMdict tag that the DTD does not declare
- each `<re_restr>`, `<stagk>`, or `<stagr>` restriction that stage 2 skips, with the `ent_seq` of
  its entry
- each KanjiVG group whose `kvg:element` value does not give exactly 1 code point after normalization
- each `mutant` row
- each mutant in `data/mutants.tsv` that is not a KanjiVG component
- size of `content.db`

## 8. Tests and CI

The tests have 3 levels:

| Level | Location | Data | Runs in |
| --- | --- | --- | --- |
| Unit tests | A `#[cfg(test)]` module in each source file | Inline fixtures | `cargo test --workspace` |
| Integration test | `tests/pipeline.rs` | `tests/fixtures/` | `cargo test --workspace` |
| Build checks | `tests/build_checks.rs` | A full build from `sources/` | CI, after a full build |

No unit test and no integration test reads `sources/` or uses the network. Appendix A lists the test
cases.

### Build checks

The build checks test properties of the real data that the fixtures cannot show. The checks are
ignored tests, so `cargo test` does not run them. To run the checks after a full build, use:

```
cargo test -p ingest --test build_checks -- --ignored
```

| Check | Condition |
| --- | --- |
| `foreign_keys_hold` | `PRAGMA foreign_key_check` returns no rows |
| `golden_component_edges_exist` | `content.db` holds each `character_component` row in `tests/golden_edges.tsv`, for example 語 → 言, 語 → 吾, 吾 → 五, 吾 → 口, 休 → 亻, and 休 → 木 |
| `one_word_for_each_jmdict_entry` | Number of `word` rows equals the number of JMdict entries |
| `components_make_no_cycle` | `character_component` rows make no cycle |
| `expected_mutants_exist` | `mutant` holds 亻 → 人, 氵 → 水, 忄 → 心, ⺗ → 心, ⻞ → 食, and 艹 → 艸 |
| `one_row_for_each_jmdict_element` | Numbers of `written_form`, `reading`, `sense`, and `gloss` rows equal the numbers of `<k_ele>`, `<r_ele>`, `<sense>`, and `<gloss>` elements in JMdict |
| `each_word_has_a_reading_and_a_sense` | Each `word` row has 1 or more `reading` rows and 1 or more `sense` rows |
| `sources_have_versions` | `source` holds JMdict, KanjiVG, and Kanji alive, and the rows of JMdict and KanjiVG have a version |

### CI

`.github/workflows/ci.yml` has 2 jobs:

| Job | Steps |
| --- | --- |
| `test` | `cargo fmt --check`, `cargo clippy`, and `cargo test --workspace` |
| `build-checks` | `fetch.sh`, a full build, and the build checks |

The workflow runs on each pull request, on each push to `main`, and each Monday. The Monday run
checks new source data. The `build-checks` job keeps the sources in the cache for 1 day,
so the job downloads the sources 1 time each day at most. The job adds the build report to the job
summary, and uploads `build-report.txt` and `content.db` as an artifact.

## Appendix A. Test cases

Each row gives the name of the test function of the case.

### `jpdag::normalize` (`crates/jpdag/src/normalize.rs`)

| Test | Input | Expected result |
| --- | --- | --- |
| `kangxi_radical_becomes_unified_ideograph` | ⼝ (U+2F1D) | 口 (U+53E3) |
| `compatibility_ideograph_becomes_unified_ideograph` | 塚 (U+FA10) | 塚 (U+585A) |
| `radical_eat_two_is_unchanged` | ⻞ (U+2EDE) | ⻞ (U+2EDE) |
| `square_era_name_gives_two_code_points` | ㍻ (U+337B) | 平成 |
| `radical_simplified_walk_is_unchanged` | ⻌ (U+2ECC) | ⻌ (U+2ECC) |
| `person_radical_ideograph_is_unchanged` | 亻 (U+4EBB) | 亻 (U+4EBB) |
| `radical_city_merges_with_its_ideograph` | ⻏ (U+2ECF) | 阝 (U+961D) |
| `radical_dog_merges_with_its_ideograph` | ⺨ (U+2EA8) | 犭 (U+72AD) |
| `radical_small_is_unchanged` | ⺌ (U+2E8C) | ⺌ (U+2E8C) |

### `jmdict.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `keeps_the_parts_of_an_entry_in_order` | An entry with 2 written forms, each with tags | `ent_seq`, the 2 written forms in order, and the tags of each written form |
| `entity_reference_gives_its_name_as_the_tag` | `<misc>&uk;</misc>` in the first sense | Tag `uk`, not the DTD text |
| `entry_without_written_forms` | An entry with no `<k_ele>` | An empty list of written forms |
| `tag_belongs_to_its_own_sense` | `uk` in the second sense only | Tag `uk` on the second sense only |
| `reads_gzip_input` | Entry of `keeps_the_parts_of_an_entry_in_order`, compressed with gzip | Same result |
| `unclosed_element_error_gives_the_line` | XML with an unclosed element | An error that gives the line number |
| `undeclared_entity_keeps_its_name` | An entity name that the DTD does not declare | Name, kept as the tag |
| `readings_keep_restrictions_no_kanji_and_tags` | An entry with 3 readings: 1 with `<re_restr>`, 1 with `<re_nokanji/>`, and 1 with a `<re_inf>` tag | Readings in order, the restriction text, the no-kanji flag, and the tag |
| `sense_keeps_restrictions_and_tags_by_category` | A sense with `<stagk>`, `<stagr>`, `<pos>`, `<field>`, `<misc>`, and `<dial>` | 2 restriction texts, and the tags of each category |
| `dtd_gives_the_entity_descriptions` | A DTD that declares `uk` as "word usually written using kana alone" | Tag `uk` with that description |
| `glosses_keep_their_order_and_type` | A sense with 2 `<gloss>` elements, the second with `g_type="lit"` | 2 gloss texts in order, and the type `lit` for the second gloss only |
| `sense_keeps_its_note` | A sense with `<s_inf>before a verb in negative form</s_inf>` | Note text of the sense |
| `creation_comment_gives_the_version` | Comment `<!-- JMdict created: 2026-09-28 -->` | Version 2026-09-28 |
| `no_creation_comment_gives_no_version` | A file with no creation comment | No version, and the build report says so |

### `words.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `ids_follow_the_word_id_and_the_position` | 2 entries, each with 2 written forms, 2 readings, and 2 senses | Rows in order of word id and position, with the positions 1 and 2 |
| `reading_restriction_links_the_named_written_form` | A reading whose `<re_restr>` names the second written form | 1 `reading_restriction` row to the second written form |
| `sense_restrictions_link_the_named_forms` | A sense with 1 `<stagk>` and 1 `<stagr>` | 1 `sense_written_form` row and 1 `sense_reading` row |
| `restriction_that_names_no_form_is_skipped` | A `<re_restr>` text that names no written form of the entry | No `reading_restriction` row, and the build report lists the restriction with the `ent_seq` |
| `sense_without_pos_takes_the_previous_pos` | A second sense with no `<pos>` | `sense_tag` rows of the second sense hold the `<pos>` tags of the first sense |
| `same_name_in_two_categories_gives_two_tags` | Name `ik` in a `<ke_inf>` and in a `<re_inf>` | 2 `tag` rows, with the categories `ke_inf` and `re_inf` |
| `undeclared_entity_gives_a_tag_without_description` | An entity that the DTD does not declare | A `tag` row with no description, and the build report lists the tag |
| `glosses_get_positions_in_their_sense` | 2 senses: the first with 3 glosses, the second with no gloss | 3 `gloss` rows with the positions 1 to 3 for the first sense, and a `sense` row with no `gloss` rows for the second |
| `sense_notes_are_joined` | A sense with 1 `<s_inf>`, a sense with 2 `<s_inf>`, and a sense with none | `sense.note` is the text, the 2 texts joined with "; ", and NULL |

### `kanji_links.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `each_written_form_links_to_its_kanji` | Written forms 一寸 and 鳥渡 | 一 and 寸 link to 一寸, and 鳥 and 渡 link to 鳥渡 |
| `tags_do_not_filter_the_links` | A written form with the tag `rK` | Kanji of the form. Tags do not filter the links. |
| `uk_does_not_filter_the_links` | A word with `uk` in the first sense | Kanji of each written form. `uk` does not filter the links. |
| `iteration_mark_is_not_a_kanji` | 人々 | 人 |
| `small_ke_is_not_a_kanji` | 一ヶ月 | 一, 月 |
| `okurigana_are_not_kanji` | 取り扱い | 取, 扱 |
| `form_without_kanji_gives_no_links` | Tシャツ | No `written_form_kanji` rows |
| `shime_mark_is_not_a_kanji` | 〆切 | 切 |
| `kanji_of_a_supplementary_plane` | 𠮟る (U+20B9F) | U+20B9F |
| `compatibility_ideograph_links_its_unified_ideograph` | U+FA19, a compatibility ideograph | U+795E (神) |
| `unified_ideograph_in_the_compatibility_block_is_kept` | 﨑 (U+FA11), a unified ideograph in the compatibility block | U+FA11 |
| `repeated_kanji_links_once` | A written form with the same kanji 2 times | 1 `written_form_kanji` row |

### `kanjivg.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `records_direct_components_only` | 語, with the components 五 and 口 inside the group of 吾 | Direct components 言 and 吾 only |
| `group_without_element_passes_to_its_children` | A group without `kvg:element` that holds 2 groups with `kvg:element` | `kvg:element` values of the 2 inner groups are direct components |
| `split_component_gives_one_component` | 2 groups with the same `kvg:element`, and the `kvg:part` values 1 and 2 | 1 direct component |
| `character_is_not_its_own_component` | A top group whose `kvg:element` is the kanji of the entry | Kanji is not a direct component of itself |
| `variant_style_entry_is_ignored` | `id="kvg:kanji_08a9e-Kaisho"` | No KanjiVG entry |
| `uses_element_not_original` | `kvg:element="亻"` with `kvg:original="人"` | Direct component 亻 |
| `element_is_normalized` | `kvg:element="⼝"` (U+2F1D) | Direct component 口 (U+53E3) |
| `entry_of_a_supplementary_plane` | `id="kvg:kanji_20b9f"` | Code point U+20B9F |
| `cdp_code_is_reported_and_its_children_visited` | `kvg:element="CDP-8BD0"` | Child groups give the direct components, and the build report lists the group |
| `nested_group_that_names_the_entry_is_skipped` | A nested group whose `kvg:element` gives the kanji of the entry after normalization | Child groups give the direct components |
| `radical_eat_two_is_a_component` | `kvg:element="⻞"` | Direct component ⻞ |
| `entry_that_normalization_changes_is_ignored` | `<kanji>` element of ⺨ | No KanjiVG entry |
| `generated_comment_gives_the_version` | Comment "This file was generated on 2025-08-16 from the most recent KanjiVG data." | Version 2025-08-16 |

### `mutants.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `reads_a_row` | A line with a mutant and a base | 1 stage 4 row |
| `skips_comments_and_blank_lines` | A line that starts with `#` | No row |
| `column_with_two_characters_fails` | A line whose first column has 2 characters | Build fails, and the error gives the line number |
| `mutant_equal_to_its_base_fails` | A line whose mutant and base are the same character after normalization | Build fails |
| `duplicate_row_fails` | 2 lines with the same mutant and base | Build fails |
| `mound_and_city_normalize_to_one_mutant` | Lines ⻖ → 阜 and ⻏ → 邑 | Rows 阝 → 阜 and 阝 → 邑 |
| `mutant_that_is_not_a_kanjivg_component_is_reported` | A mutant that is not a KanjiVG component | 1 stage 4 row, and the build report lists the mutant |

### `components.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `kanji_with_an_entry_gets_its_components` | A kanji of a word with a KanjiVG entry | Direct components of the KanjiVG entry |
| `component_without_an_entry_is_uncovered` | A KanjiVG entry with a direct component P, where P has no KanjiVG entry | Direct components of the entry. P is an uncovered character, and the build report lists P. |
| `kanji_without_an_entry_is_uncovered` | A kanji of a word with no KanjiVG entry | An uncovered character that the build report lists |
| `components_of_a_component_join_the_set` | A direct component with a KanjiVG entry | Direct components of that component are in the set |
| `base_of_a_mutant_joins_the_set` | A character in the set that is the mutant of a stage 4 row | Base is in the set |
| `mutant_outside_the_set_gives_no_row` | A stage 4 row whose mutant is not in the set | No `mutant` row |

### `db.rs`

| Test | Input | Expected result |
| --- | --- | --- |
| `empty_build_creates_the_schema` | A build with no rows | 17 tables and the indexes of section 5. `PRAGMA user_version` equals `SCHEMA_VERSION`. |
| `rows_round_trip` | A small set of rows | Same rows in each table |
| `foreign_key_violation_fails_the_build` | A `written_form_kanji` row whose `kanji_id` has no `character` row | Build fails, and no `content.db` exists |
| `failed_build_keeps_the_previous_db` | A failure after the insert, with a previous `content.db` | Previous `content.db` is unchanged, and no `content.db.tmp` exists |

### Integration test (`tests/pipeline.rs`)

| Test | Input | Expected result |
| --- | --- | --- |
| `fixture_rows_equal_expected` | Fixture sources in `tests/fixtures/` | Rows of the 17 tables equal the rows in `tests/fixtures/expected.tsv` |
| `two_runs_give_the_same_rows` | 2 runs on the same fixtures | Same rows in each table |
| `report_counts_match_the_fixtures` | Fixture sources | Counts in the build report that match the fixtures |

The fixtures hold:

- 1 JMdict entry for each case of `kanji_links.rs`
- JMdict entries with readings, senses, glosses, restrictions, and tags of each category, for the
  cases of `words.rs`
- KanjiVG entries of 語, 吾, and 休
- 1 KanjiVG entry with a direct component that has no KanjiVG entry
- 1 KanjiVG entry with the direct component ⻞
- 1 kanji of a word with no KanjiVG entry
