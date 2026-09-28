//! The integration test: runs the ingest binary on the fixture sources in `tests/fixtures/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;
use rusqlite::types::ValueRef;

/// The tables of `content.db`, in the order of the schema.
const TABLES: [&str; 17] = [
    "word",
    "character",
    "character_component",
    "kanji_form_kanji",
    "mutant",
    "kanji_form",
    "reading",
    "reading_restriction",
    "sense",
    "gloss",
    "sense_kanji_form",
    "sense_reading",
    "tag",
    "kanji_form_tag",
    "reading_tag",
    "sense_tag",
    "source",
];

/// The columns that hold a code point. The dump shows the character.
const CHARACTER_COLUMNS: [(&str, &str); 7] = [
    ("character", "id"),
    ("character_component", "character_id"),
    ("character_component", "component_id"),
    ("kanji_form_kanji", "kanji_id"),
    ("mutant", "mutant_id"),
    ("mutant", "base_id"),
    ("source", "attribution"),
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Runs the ingest binary with the output directory `out`, and gives the build report.
fn run_ingest(out: &Path) -> String {
    let _ = fs::remove_dir_all(out);
    let output = Command::new(env!("CARGO_BIN_EXE_ingest"))
        .arg("--jmdict")
        .arg(fixture("JMdict_e.xml"))
        .arg("--kanjivg")
        .arg(fixture("kanjivg.xml"))
        .arg("--out")
        .arg(out)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read_to_string(out.join("build-report.txt")).unwrap()
}

fn out_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

/// The rows of `content.db` in the format of `expected.tsv`, sorted: the table, then the value of
/// each column. A code point is shown as its character, and the attribution text as `…`.
fn dump(db: &Path) -> Vec<String> {
    let conn = Connection::open(db).unwrap();
    let mut rows = Vec::new();
    for table in TABLES {
        let mut stmt = conn.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        let mut query = stmt.query([]).unwrap();
        while let Some(row) = query.next().unwrap() {
            let mut line = table.to_owned();
            for (i, column) in columns.iter().enumerate() {
                let value = match row.get_ref(i).unwrap() {
                    ValueRef::Null => "NULL".to_owned(),
                    ValueRef::Integer(n) => {
                        if CHARACTER_COLUMNS.contains(&(table, column)) {
                            char::from_u32(n as u32).unwrap().to_string()
                        } else {
                            n.to_string()
                        }
                    }
                    ValueRef::Text(t) => {
                        if CHARACTER_COLUMNS.contains(&(table, column)) {
                            "…".to_owned()
                        } else {
                            String::from_utf8(t.to_vec()).unwrap()
                        }
                    }
                    other => panic!("unexpected value {other:?} in {table}.{column}"),
                };
                line.push('\t');
                line.push_str(&value);
            }
            rows.push(line);
        }
    }
    rows.sort();
    rows
}

fn expected() -> Vec<String> {
    let mut rows: Vec<String> = fs::read_to_string(fixture("expected.tsv"))
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect();
    rows.sort();
    rows
}

#[test]
fn fixture_rows_equal_expected() {
    let out = out_dir("i1");
    run_ingest(&out);
    assert_eq!(dump(&out.join("content.db")), expected());
}

#[test]
fn two_runs_give_the_same_rows() {
    let (first, second) = (out_dir("i2-first"), out_dir("i2-second"));
    run_ingest(&first);
    run_ingest(&second);
    assert_eq!(
        dump(&first.join("content.db")),
        dump(&second.join("content.db"))
    );
}

#[test]
fn report_counts_match_the_fixtures() {
    let report = run_ingest(&out_dir("i3"));
    let count = |table: &str| {
        expected()
            .iter()
            .filter(|row| row.split('\t').next() == Some(table))
            .count()
    };
    for table in TABLES {
        let line = format!("  {table:<20} {}", count(table));
        assert!(
            report.lines().any(|l| l == line),
            "no line {line:?} in the report:\n{report}"
        );
    }
    for line in [
        "  JMdict               2026-09-28",
        "  KanjiVG              2025-08-16",
        "  Kanji alive          none (data/mutants.tsv holds its data)",
        "Characters with a KanjiVG entry: 11",
        "Uncovered characters: 40",
        "  𠮟 U+20B9F",
        "  𠂉 U+20089",
        "JMdict tags that the DTD does not declare: 1",
        "  misc newtag",
        "Restrictions that name no form of their entry: 2",
        "  ent_seq 1000520: <re_restr>掌",
        "  ent_seq 1000520: <stagk>腕",
        "KanjiVG groups whose kvg:element value does not give exactly 1 code point after normalization: 0",
    ] {
        assert!(
            report.lines().any(|l| l == line),
            "no line {line:?} in the report:\n{report}"
        );
    }
}
