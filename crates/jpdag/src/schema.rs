//! Schema of `content.db`.

/// Value of `PRAGMA user_version` in a `content.db` with the tables of [`CREATE_TABLES`].
pub const SCHEMA_VERSION: i32 = 7;

/// The last frequency rank that orders words. Many words past it share a rank, because they occur
/// a few times each in the dramas of Jiten, so their ranks are noise: lists treat them as 1 band of
/// rare words, after the words up to this rank and before the words with no rank.
pub const COMMON_RANK: u32 = 20_000;

/// Tables and indexes of `content.db`, as `Docs/ingest-design.md` gives them.
pub const CREATE_TABLES: &str = "
CREATE TABLE word (
  id INTEGER PRIMARY KEY                                    -- JMdict ent_seq
) STRICT;

CREATE TABLE character (
  id INTEGER PRIMARY KEY                                    -- code point after normalization
) STRICT;

CREATE TABLE character_meaning (
  character_id INTEGER NOT NULL REFERENCES character(id),
  position     INTEGER NOT NULL,
  text         TEXT NOT NULL,
  PRIMARY KEY (character_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE character_reading (
  character_id INTEGER NOT NULL REFERENCES character(id),
  position     INTEGER NOT NULL,
  type         TEXT NOT NULL CHECK (type IN ('on', 'kun')),
  text         TEXT NOT NULL,
  word_count   INTEGER NOT NULL,                            -- words that use the reading
  common_word_count INTEGER NOT NULL,                       -- of those, the words with a rank
                                                            -- up to COMMON_RANK
  PRIMARY KEY (character_id, position)
) STRICT, WITHOUT ROWID;

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

CREATE TABLE furigana (
  written_form_id INTEGER NOT NULL REFERENCES written_form(id),
  reading_id      INTEGER NOT NULL REFERENCES reading(id),
  position        INTEGER NOT NULL,
  start           INTEGER NOT NULL,                         -- index of the first character
  length          INTEGER NOT NULL,                         -- number of characters
  kana            TEXT NOT NULL,
  PRIMARY KEY (written_form_id, reading_id, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE accent (
  id         INTEGER PRIMARY KEY,
  reading_id INTEGER NOT NULL REFERENCES reading(id),
  position   INTEGER NOT NULL,
  downstep   INTEGER NOT NULL,                              -- morae before the fall in pitch,
                                                            -- or 0 for no fall
  UNIQUE (reading_id, position)
) STRICT;

CREATE TABLE accent_tag (
  accent_id INTEGER NOT NULL REFERENCES accent(id),
  tag_id    INTEGER NOT NULL REFERENCES tag(id),
  PRIMARY KEY (accent_id, tag_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE written_form_frequency (
  written_form_id INTEGER PRIMARY KEY REFERENCES written_form(id),
  rank            INTEGER NOT NULL                          -- of the word in this written form
) STRICT;

CREATE TABLE reading_frequency (
  reading_id INTEGER PRIMARY KEY REFERENCES reading(id),
  rank       INTEGER NOT NULL                               -- of the word in kana, as this reading
) STRICT;

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
  name        TEXT PRIMARY KEY,                             -- JMdict, KanjiVG, Kanji alive, KANJIDIC2,
                                                            -- JmdictFurigana, Kanjium, or Jiten
  version     TEXT,                                         -- the file's date, or a commit
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
";
