//! Build checks on a full build from `sources/`. `cargo test` does not run these
//! checks. To run them after a full build, use:
//!
//! cargo test -p ingest --test build_checks -- --ignored
//!
//! Environment variable `CONTENT_DB` gives the path of `content.db`, and `JMDICT` gives the path
//! of `JMdict_e.gz`. The defaults are `out/content.db` and `sources/JMdict_e.gz` in the crate.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

fn path(variable: &str, default: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(default))
}

fn content_db() -> Connection {
    let db = path("CONTENT_DB", "out/content.db");
    assert!(
        db.exists(),
        "{} does not exist; run a full build first",
        db.display()
    );
    Connection::open(db).unwrap()
}

fn pairs(conn: &Connection, sql: &str) -> Vec<(char, char)> {
    conn.prepare(sql)
        .unwrap()
        .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)))
        .unwrap()
        .map(|r| {
            let (a, b) = r.unwrap();
            (char::from_u32(a).unwrap(), char::from_u32(b).unwrap())
        })
        .collect()
}

#[test]
#[ignore]
fn foreign_keys_hold() {
    let conn = content_db();
    let violations: usize = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |_| Ok(()))
        .unwrap()
        .count();
    assert_eq!(violations, 0);
}

#[test]
#[ignore]
fn golden_component_edges_exist() {
    let conn = content_db();
    let edges: HashSet<(char, char)> = pairs(
        &conn,
        "SELECT character_id, component_id FROM character_component",
    )
    .into_iter()
    .collect();
    let golden =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden_edges.tsv"))
            .unwrap();
    let missing: Vec<&str> = golden
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter(|line| {
            let mut fields = line.split('\t').map(|f| f.chars().next().unwrap());
            !edges.contains(&(fields.next().unwrap(), fields.next().unwrap()))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "missing character_component rows: {missing:?}"
    );
}

/// Body of `JMdict_e.gz`: the decompressed text after the DTD, whose comments name the
/// elements too. The checks count the elements without the parser of the ingest crate.
fn jmdict_xml() -> String {
    let mut xml = String::new();
    flate2::read::MultiGzDecoder::new(
        fs::File::open(path("JMDICT", "sources/JMdict_e.gz")).unwrap(),
    )
    .read_to_string(&mut xml)
    .unwrap();
    let body = xml.find("]>").map_or(0, |i| i + 2);
    xml.split_off(body)
}

fn count(conn: &Connection, sql: &str) -> usize {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap() as usize
}

#[test]
#[ignore]
fn one_word_for_each_jmdict_entry() {
    let entries = jmdict_xml().matches("<entry>").count();
    assert_eq!(count(&content_db(), "SELECT count(*) FROM word"), entries);
}

#[test]
#[ignore]
fn components_make_no_cycle() {
    let edges = pairs(
        &content_db(),
        "SELECT character_id, component_id FROM character_component",
    );
    let mut components: HashMap<char, Vec<char>> = HashMap::new();
    for &(c, d) in &edges {
        components.entry(c).or_default().push(d);
    }
    // A depth-first search. A character on the current path that occurs again closes a cycle.
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        OnPath,
        Done,
    }
    let mut state: HashMap<char, State> = HashMap::new();
    for &start in components.keys() {
        if state.contains_key(&start) {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state.insert(start, State::OnPath);
        while let Some(&mut (c, ref mut next)) = stack.last_mut() {
            let children = components.get(&c).map_or(&[][..], Vec::as_slice);
            if let Some(&d) = children.get(*next) {
                *next += 1;
                match state.get(&d) {
                    Some(State::OnPath) => {
                        let path: String = stack.iter().map(|&(c, _)| c).collect();
                        panic!("cycle: {path}{d}");
                    }
                    Some(State::Done) => {}
                    None => {
                        state.insert(d, State::OnPath);
                        stack.push((d, 0));
                    }
                }
            } else {
                state.insert(c, State::Done);
                stack.pop();
            }
        }
    }
}

#[test]
#[ignore]
fn expected_mutants_exist() {
    let rows: HashSet<(char, char)> = pairs(&content_db(), "SELECT mutant_id, base_id FROM mutant")
        .into_iter()
        .collect();
    for row in [
        ('亻', '人'),
        ('氵', '水'),
        ('忄', '心'),
        ('⺗', '心'),
        ('⻞', '食'),
        ('艹', '艸'),
    ] {
        assert!(
            rows.contains(&row),
            "mutant does not hold {} → {}",
            row.0,
            row.1
        );
    }
}

#[test]
#[ignore]
fn one_row_for_each_jmdict_element() {
    let xml = jmdict_xml();
    let elements = |names: &[&str]| names.iter().map(|n| xml.matches(n).count()).sum::<usize>();
    let conn = content_db();
    for (table, names) in [
        ("kanji_form", &["<k_ele>"][..]),
        ("reading", &["<r_ele>"]),
        ("sense", &["<sense>", "<sense/>"]),
        ("gloss", &["<gloss>", "<gloss "]),
    ] {
        assert_eq!(
            count(&conn, &format!("SELECT count(*) FROM {table}")),
            elements(names),
            "{table}"
        );
    }
}

#[test]
#[ignore]
fn each_word_has_a_reading_and_a_sense() {
    let conn = content_db();
    for table in ["reading", "sense"] {
        let missing = count(
            &conn,
            &format!("SELECT count(*) FROM word WHERE id NOT IN (SELECT word_id FROM {table})"),
        );
        assert_eq!(missing, 0, "words with no {table} row");
    }
}

#[test]
#[ignore]
fn sources_have_versions() {
    let conn = content_db();
    let sources: HashMap<String, Option<String>> = conn
        .prepare("SELECT name, version FROM source")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    for name in ["JMdict", "KanjiVG", "Kanji alive"] {
        assert!(sources.contains_key(name), "no source row for {name}");
    }
    for name in ["JMdict", "KanjiVG"] {
        assert!(
            sources[name].is_some(),
            "the source row of {name} has no version"
        );
    }
}
