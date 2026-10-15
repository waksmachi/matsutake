//! Integration test: runs the ingest binary on the fixture sources in `tests/fixtures/`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;
use rusqlite::types::ValueRef;

/// Tables of `content.db`, in schema order.
const TABLES: [&str; 24] = [
    "word",
    "character",
    "character_meaning",
    "character_reading",
    "character_component",
    "written_form_kanji",
    "mutant",
    "written_form",
    "reading",
    "reading_restriction",
    "sense",
    "gloss",
    "sense_written_form",
    "sense_reading",
    "furigana",
    "accent",
    "accent_tag",
    "written_form_frequency",
    "reading_frequency",
    "tag",
    "written_form_tag",
    "reading_tag",
    "sense_tag",
    "source",
];

/// Columns that hold a code point. The dump shows the character.
const CHARACTER_COLUMNS: [(&str, &str); 9] = [
    ("character", "id"),
    ("character_meaning", "character_id"),
    ("character_reading", "character_id"),
    ("character_component", "character_id"),
    ("character_component", "component_id"),
    ("written_form_kanji", "kanji_id"),
    ("mutant", "mutant_id"),
    ("mutant", "base_id"),
    ("source", "attribution"),
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Zips the files of the fixture `jiten-drama/`, as Jiten publishes its Yomitan dictionary, and
/// gives the path of the zip.
fn jiten_zip(out: &Path) -> PathBuf {
    let path = out.with_extension("zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for name in ["index.json", "term_meta_bank_1.json"] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&fs::read(fixture("jiten-drama").join(name)).unwrap())
            .unwrap();
    }
    zip.finish().unwrap();
    path
}

/// Runs the ingest binary with the output directory `out`, and gives the build report.
fn run_ingest(out: &Path) -> String {
    let _ = fs::remove_dir_all(out);
    let jiten = jiten_zip(out);
    let output = Command::new(env!("CARGO_BIN_EXE_ingest"))
        .arg("--jmdict")
        .arg(fixture("JMdict_e.xml"))
        .arg("--kanjivg")
        .arg(fixture("kanjivg.xml"))
        .arg("--kanjidic")
        .arg(fixture("kanjidic2.xml"))
        .arg("--furigana")
        .arg(fixture("JmdictFurigana-2026-09-25.txt"))
        .arg("--accents")
        .arg(fixture("kanjium-accents-685d4d7.txt"))
        .arg("--frequency")
        .arg(jiten)
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

/// Rows of `content.db` in the format of `expected.tsv`, sorted: the table, then the value of
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
        "  KANJIDIC2            2026-10-01",
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
        "Characters with a KANJIDIC2 entry: 3",
        "KANJIDIC2 entries that stage 6 ignores: 2",
        "Mutants with no KANJIDIC2 entry: 7",
        "  亻 U+4EBB",
        "Characters with a KANJIDIC2 entry and no English meaning: 0",
        "  JmdictFurigana       2026-09-25",
        "JmdictFurigana lines: 5",
        "JmdictFurigana lines that are not a written form and a reading of 1 word: 1",
        "Furigana of 1 kanji that are none of its KANJIDIC2 readings: 0",
        "KANJIDIC2 readings that 1 or more words use: 3",
        "  Kanjium              685d4d7",
        "Kanjium lines: 9",
        "Kanjium lines that give no reading an accent: 1",
        "Accents after the last mora of their reading: 0",
        "Readings with an accent: 9",
        "Readings that take the accents of another reading of their word: 1",
        "  Jiten                2026-10-09",
        "Jiten entries: 12",
        "Jiten entries that are no written form and reading of a word: 2",
        "Jiten entries of 2 or more words that go to the 1 that JMdict marks as common: 0",
        "Jiten entries of 2 or more words that go to no word: 1",
        "Words with a rank: 4",
    ] {
        assert!(
            report.lines().any(|l| l == line),
            "no line {line:?} in the report:\n{report}"
        );
    }
}
