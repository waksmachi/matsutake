//! Stage 7: write `content.db`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use jpdag::schema::{CREATE_TABLES, SCHEMA_VERSION};
use rusqlite::{Connection, Transaction, params};

use crate::words::EntryRows;

/// A `source` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: &'static str,
    pub version: Option<String>,
    pub licence: &'static str,
    pub attribution: &'static str,
}

/// Rows of the 19 tables. Each character is a code point.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Rows {
    /// Rows of `word`, the entry tables, and the tag tables.
    pub entries: EntryRows,
    /// Each row as (written_form_id, kanji_id).
    pub written_form_kanji: Vec<(u32, u32)>,
    pub characters: Vec<u32>,
    /// Each row as (character_id, position, text).
    pub character_meanings: Vec<(u32, u32, String)>,
    /// Each row as (character_id, position, type, text).
    pub character_readings: Vec<(u32, u32, &'static str, String)>,
    /// Each row as (character_id, component_id).
    pub character_components: Vec<(u32, u32)>,
    /// Each row as (mutant_id, base_id).
    pub mutants: Vec<(u32, u32)>,
    pub sources: Vec<Source>,
}

impl Rows {
    /// Number of rows in each table, in schema order.
    pub fn counts(&self) -> [(&'static str, usize); 19] {
        let e = &self.entries;
        [
            ("word", e.words.len()),
            ("character", self.characters.len()),
            ("character_meaning", self.character_meanings.len()),
            ("character_reading", self.character_readings.len()),
            ("character_component", self.character_components.len()),
            ("written_form_kanji", self.written_form_kanji.len()),
            ("mutant", self.mutants.len()),
            ("written_form", e.written_forms.len()),
            ("reading", e.readings.len()),
            ("reading_restriction", e.reading_restrictions.len()),
            ("sense", e.senses.len()),
            ("gloss", e.glosses.len()),
            ("sense_written_form", e.sense_written_forms.len()),
            ("sense_reading", e.sense_readings.len()),
            ("tag", e.tags.len()),
            ("written_form_tag", e.written_form_tags.len()),
            ("reading_tag", e.reading_tags.len()),
            ("sense_tag", e.sense_tags.len()),
            ("source", self.sources.len()),
        ]
    }
}

/// Writes `content.db` in `out_dir`, and gives the size of the file in bytes. A failed write
/// leaves the previous `content.db` unchanged.
pub fn write(out_dir: &Path, rows: &Rows) -> Result<u64> {
    let tmp = out_dir.join("content.db.tmp");
    let db = out_dir.join("content.db");
    let result = write_tmp(&tmp, rows).and_then(|()| {
        fs::rename(&tmp, &db).with_context(|| format!("cannot rename {}", tmp.display()))
    });
    if let Err(error) = result {
        remove_tmp(&tmp);
        return Err(error);
    }
    Ok(fs::metadata(&db)?.len())
}

fn remove_tmp(tmp: &Path) {
    let mut journal = PathBuf::from(tmp);
    journal.set_extension("tmp-journal");
    for path in [tmp, &journal] {
        let _ = fs::remove_file(path);
    }
}

fn insert_pairs(tx: &Transaction, table: &str, pairs: &[(u32, u32)]) -> Result<()> {
    let mut insert = tx.prepare(&format!("INSERT INTO {table} VALUES (?1, ?2)"))?;
    for (a, b) in pairs {
        insert.execute([a, b])?;
    }
    Ok(())
}

fn insert_rows(tx: &Transaction, rows: &Rows) -> Result<()> {
    let e = &rows.entries;
    let mut insert = tx.prepare("INSERT INTO word (id) VALUES (?1)")?;
    for id in &e.words {
        insert.execute([id])?;
    }
    let mut insert = tx.prepare("INSERT INTO character (id) VALUES (?1)")?;
    for id in &rows.characters {
        insert.execute([id])?;
    }
    let mut insert = tx.prepare("INSERT INTO character_meaning VALUES (?1, ?2, ?3)")?;
    for (character_id, position, text) in &rows.character_meanings {
        insert.execute(params![character_id, position, text])?;
    }
    let mut insert = tx.prepare("INSERT INTO character_reading VALUES (?1, ?2, ?3, ?4)")?;
    for (character_id, position, kind, text) in &rows.character_readings {
        insert.execute(params![character_id, position, kind, text])?;
    }
    let mut insert = tx.prepare("INSERT INTO written_form VALUES (?1, ?2, ?3, ?4)")?;
    for k in &e.written_forms {
        insert.execute(params![k.id, k.word_id, k.position, k.text])?;
    }
    let mut insert = tx.prepare("INSERT INTO reading VALUES (?1, ?2, ?3, ?4, ?5)")?;
    for r in &e.readings {
        insert.execute(params![r.id, r.word_id, r.position, r.text, r.no_kanji])?;
    }
    let mut insert = tx.prepare("INSERT INTO sense VALUES (?1, ?2, ?3, ?4)")?;
    for s in &e.senses {
        insert.execute(params![s.id, s.word_id, s.position, s.note])?;
    }
    let mut insert = tx.prepare("INSERT INTO gloss VALUES (?1, ?2, ?3, ?4)")?;
    for g in &e.glosses {
        insert.execute(params![g.sense_id, g.position, g.text, g.g_type])?;
    }
    let mut insert = tx.prepare("INSERT INTO tag VALUES (?1, ?2, ?3, ?4)")?;
    for t in &e.tags {
        insert.execute(params![t.id, t.category.as_str(), t.name, t.description])?;
    }
    let mut insert = tx.prepare("INSERT INTO source VALUES (?1, ?2, ?3, ?4)")?;
    for s in &rows.sources {
        insert.execute(params![s.name, s.version, s.licence, s.attribution])?;
    }
    for (table, pairs) in [
        ("character_component", &rows.character_components),
        ("written_form_kanji", &rows.written_form_kanji),
        ("mutant", &rows.mutants),
        ("reading_restriction", &e.reading_restrictions),
        ("sense_written_form", &e.sense_written_forms),
        ("sense_reading", &e.sense_readings),
        ("written_form_tag", &e.written_form_tags),
        ("reading_tag", &e.reading_tags),
        ("sense_tag", &e.sense_tags),
    ] {
        insert_pairs(tx, table, pairs)?;
    }
    Ok(())
}

fn write_tmp(tmp: &Path, rows: &Rows) -> Result<()> {
    remove_tmp(tmp);
    let mut conn =
        Connection::open(tmp).with_context(|| format!("cannot open {}", tmp.display()))?;
    // The bundled SQLite enforces foreign keys by default. Step 5 checks them after the insert.
    conn.pragma_update(None, "foreign_keys", false)?;
    conn.execute_batch(CREATE_TABLES)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;

    let tx = conn.transaction()?;
    insert_rows(&tx, rows)?;
    tx.commit()?;

    let violations: Vec<String> = conn
        .prepare("PRAGMA foreign_key_check")?
        .query_map([], |row| {
            Ok(format!(
                "{} references {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(2)?
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    if let Some(first) = violations.first() {
        bail!(
            "foreign key check failed: {} rows, for example a row of {first} that does not exist",
            violations.len()
        );
    }

    conn.execute_batch("VACUUM")?;
    conn.close().map_err(|(_, e)| e)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jmdict::Category;
    use crate::words::{GlossRow, ReadingRow, SenseRow, TagRow, WrittenFormRow};

    fn out_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ingest-db-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(conn: &Connection, kind: &str) -> Vec<String> {
        conn.prepare("SELECT name FROM sqlite_master WHERE type = ?1 AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([kind], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        conn.prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn strings(conn: &Connection, sql: &str) -> Vec<String> {
        conn.prepare(sql)
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn small_rows() -> Rows {
        Rows {
            entries: EntryRows {
                words: vec![1000, 1001],
                written_forms: vec![WrittenFormRow {
                    id: 1,
                    word_id: 1000,
                    position: 1,
                    text: "休む".into(),
                }],
                readings: vec![ReadingRow {
                    id: 1,
                    word_id: 1000,
                    position: 1,
                    text: "やすむ".into(),
                    no_kanji: false,
                }],
                reading_restrictions: vec![(1, 1)],
                senses: vec![SenseRow {
                    id: 1,
                    word_id: 1000,
                    position: 1,
                    note: Some("a note".into()),
                }],
                glosses: vec![GlossRow {
                    sense_id: 1,
                    position: 1,
                    text: "to rest".into(),
                    g_type: None,
                }],
                sense_written_forms: vec![(1, 1)],
                sense_readings: vec![(1, 1)],
                tags: vec![TagRow {
                    id: 1,
                    category: Category::Pos,
                    name: "v5m".into(),
                    description: None,
                }],
                written_form_tags: vec![(1, 1)],
                reading_tags: vec![(1, 1)],
                sense_tags: vec![(1, 1)],
                skipped_restrictions: vec![],
            },
            written_form_kanji: vec![(1, 0x4F11)],
            characters: vec![0x4EBA, 0x4EBB, 0x4F11, 0x6728],
            character_meanings: vec![(0x4F11, 1, "rest".into()), (0x4F11, 2, "day off".into())],
            character_readings: vec![
                (0x4F11, 1, "on", "キュウ".into()),
                (0x4F11, 2, "kun", "やす.む".into()),
            ],
            character_components: vec![(0x4F11, 0x4EBB), (0x4F11, 0x6728)],
            mutants: vec![(0x4EBB, 0x4EBA)],
            sources: vec![Source {
                name: "JMdict",
                version: Some("2026-09-28".into()),
                licence: "CC BY-SA 4.0",
                attribution: "EDRDG",
            }],
        }
    }

    #[test]
    fn empty_build_creates_the_schema() {
        let dir = out_dir("d1");
        write(&dir, &Rows::default()).unwrap();
        let conn = Connection::open(dir.join("content.db")).unwrap();
        let tables = names(&conn, "table");
        assert_eq!(tables.len(), 19, "{tables:?}");
        assert_eq!(names(&conn, "index").len(), 8);
        assert_eq!(columns(&conn, "mutant"), ["mutant_id", "base_id"]);
        assert_eq!(
            columns(&conn, "character_reading"),
            ["character_id", "position", "type", "text"]
        );
        assert_eq!(
            columns(&conn, "reading"),
            ["id", "word_id", "position", "text", "no_kanji"]
        );
        let version: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn rows_round_trip() {
        let dir = out_dir("d2");
        let rows = small_rows();
        write(&dir, &rows).unwrap();
        let conn = Connection::open(dir.join("content.db")).unwrap();
        for (table, count) in rows.counts() {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n as usize, count, "{table}");
        }
        assert_eq!(strings(&conn, "SELECT text FROM written_form"), ["休む"]);
        assert_eq!(strings(&conn, "SELECT note FROM sense"), ["a note"]);
        assert_eq!(strings(&conn, "SELECT category FROM tag"), ["pos"]);
        assert_eq!(
            strings(
                &conn,
                "SELECT text FROM character_meaning ORDER BY position"
            ),
            ["rest", "day off"]
        );
        assert_eq!(
            strings(
                &conn,
                "SELECT type FROM character_reading ORDER BY position"
            ),
            ["on", "kun"]
        );
        assert_eq!(strings(&conn, "SELECT version FROM source"), ["2026-09-28"]);
    }

    #[test]
    fn foreign_key_violation_fails_the_build() {
        let dir = out_dir("d3");
        let mut rows = small_rows();
        rows.written_form_kanji.push((1, 0x8A9E));
        let error = write(&dir, &rows).unwrap_err().to_string();
        assert!(error.contains("foreign key"), "{error}");
        assert!(!dir.join("content.db").exists());
        assert!(!dir.join("content.db.tmp").exists());
    }

    #[test]
    fn failed_build_keeps_the_previous_db() {
        let dir = out_dir("d4");
        write(&dir, &small_rows()).unwrap();
        let previous = fs::read(dir.join("content.db")).unwrap();

        let mut rows = small_rows();
        rows.written_form_kanji.push((1, 0x8A9E));
        assert!(write(&dir, &rows).is_err());
        assert_eq!(fs::read(dir.join("content.db")).unwrap(), previous);
        assert!(!dir.join("content.db.tmp").exists());
    }
}
