//! Builds the tables of `content.db` from JMdict, KanjiVG, `data/mutants.tsv`, and KANJIDIC2.
//! `Docs/ingest-design.md` describes the pipeline.

mod components;
mod db;
mod jmdict;
mod kanji_links;
mod kanjidic;
mod kanjivg;
mod mutants;
mod source;
mod words;

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: ingest --jmdict JMdict_e.gz --kanjivg kanjivg-YYYYMMDD.xml.gz \
--kanjidic kanjidic2.xml.gz --out DIR";

const JMDICT_ATTRIBUTION: &str = "This app uses the JMdict dictionary file. The file is the \
property of the Electronic Dictionary Research and Development Group, and is used in conformance \
with the licence of the Group. https://www.edrdg.org/edrdg/licence.html";

const KANJIVG_ATTRIBUTION: &str = "This app uses the character data of KanjiVG, copyright (C) \
2009-2013 Ulrich Apel, under the Creative Commons Attribution-Share Alike 3.0 licence. \
https://kanjivg.tagaini.net";

const KANJI_ALIVE_ATTRIBUTION: &str = "This app uses mutant data from Kanji alive, under the \
Creative Commons Attribution 4.0 licence. https://kanjialive.com";

const KANJIDIC_ATTRIBUTION: &str = "This app uses the KANJIDIC2 dictionary file. The file is the \
property of the Electronic Dictionary Research and Development Group, and is used in conformance \
with the licence of the Group. https://www.edrdg.org/edrdg/licence.html";

struct Args {
    jmdict: PathBuf,
    kanjivg: PathBuf,
    kanjidic: PathBuf,
    out: PathBuf,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args_os().skip(1);
    let (mut jmdict, mut kanjivg, mut kanjidic, mut out) = (None, None, None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.to_str() {
            Some("--jmdict") => &mut jmdict,
            Some("--kanjivg") => &mut kanjivg,
            Some("--kanjidic") => &mut kanjidic,
            Some("--out") => &mut out,
            _ => bail!("unknown argument {flag:?}\n{USAGE}"),
        };
        *slot = Some(PathBuf::from(
            args.next()
                .with_context(|| format!("{flag:?} needs a value"))?,
        ));
    }
    let need = |path: Option<PathBuf>, flag: &str| {
        path.with_context(|| format!("{flag} is missing\n{USAGE}"))
    };
    Ok(Args {
        jmdict: need(jmdict, "--jmdict")?,
        kanjivg: need(kanjivg, "--kanjivg")?,
        kanjidic: need(kanjidic, "--kanjidic")?,
        out: need(out, "--out")?,
    })
}

/// Data of the build report, in addition to the rows.
struct Findings {
    set: components::CharacterSet,
    kvg_unresolved: Vec<kanjivg::UnresolvedGroup>,
    not_kanjivg_components: Vec<char>,
    /// Kanji of the written forms.
    form_kanji: BTreeSet<char>,
    kanjidic: kanjidic::Kanjidic,
}

fn run(args: &Args) -> Result<String> {
    // Stage 1.
    let jmdict = jmdict::parse(source::open_path(&args.jmdict)?)?;

    // Stage 2.
    let entries = words::build(&jmdict);
    let kanji_links = kanji_links::link(&entries.written_forms);

    // Stage 3.
    let kvg = kanjivg::parse(source::open_path(&args.kanjivg)?)?;

    // Stage 4.
    let stage4 = mutants::read(mutants::MUTANTS)?;
    let kanjivg_components: HashSet<char> = kvg.entries.values().flatten().copied().collect();
    let not_kanjivg_components = mutants::not_kanjivg_components(&stage4, &kanjivg_components);

    // Stage 5.
    let set = components::complete(kanji_links.iter().map(|&(_, k)| k), &kvg.entries, &stage4);

    // Stage 6.
    let kanjidic = kanjidic::parse(source::open_path(&args.kanjidic)?, &set.characters)?;

    // Stage 7.
    let pair = |&(a, b): &(char, char)| (a as u32, b as u32);
    let rows = db::Rows {
        entries,
        written_form_kanji: kanji_links.iter().map(|&(id, k)| (id, k as u32)).collect(),
        characters: set.characters.iter().map(|&c| c as u32).collect(),
        character_meanings: kanjidic.meaning_rows(),
        character_readings: kanjidic.reading_rows(),
        character_components: set.character_components.iter().map(pair).collect(),
        mutants: set.mutants.iter().map(pair).collect(),
        sources: vec![
            db::Source {
                name: "JMdict",
                version: jmdict.version.clone(),
                licence: "CC BY-SA 4.0",
                attribution: JMDICT_ATTRIBUTION,
            },
            db::Source {
                name: "KanjiVG",
                version: kvg.version.clone(),
                licence: "CC BY-SA 3.0",
                attribution: KANJIVG_ATTRIBUTION,
            },
            db::Source {
                name: "Kanji alive",
                version: None,
                licence: "CC BY 4.0",
                attribution: KANJI_ALIVE_ATTRIBUTION,
            },
            db::Source {
                name: "KANJIDIC2",
                version: kanjidic.version.clone(),
                licence: "CC BY-SA 4.0",
                attribution: KANJIDIC_ATTRIBUTION,
            },
        ],
    };
    fs::create_dir_all(&args.out)
        .with_context(|| format!("cannot create {}", args.out.display()))?;
    let size = db::write(&args.out, &rows)?;

    let findings = Findings {
        set,
        kvg_unresolved: kvg.unresolved,
        not_kanjivg_components,
        form_kanji: kanji_links.iter().map(|&(_, k)| k).collect(),
        kanjidic,
    };
    Ok(report(&rows, &findings, size))
}

fn describe(c: char) -> String {
    format!("{c} U+{:04X}", c as u32)
}

fn report(rows: &db::Rows, findings: &Findings, size: u64) -> String {
    let set = &findings.set;
    let mut r = String::new();
    let _ = writeln!(r, "content.db build report\n");

    let _ = writeln!(r, "Sources");
    for source in &rows.sources {
        let version = match (&source.version, source.name) {
            (Some(version), _) => version.clone(),
            (None, "Kanji alive") => "none (data/mutants.tsv holds its data)".into(),
            (None, "KANJIDIC2") => "none: the file has no <date_of_creation> element".into(),
            (None, _) => "none: the file has no version comment".into(),
        };
        let _ = writeln!(r, "  {:<20} {version}", source.name);
    }

    let _ = writeln!(r, "\nRows");
    for (table, count) in rows.counts() {
        let _ = writeln!(r, "  {table:<20} {count}");
    }
    let _ = writeln!(r, "\nCharacters with a KanjiVG entry: {}", set.with_entry);
    let _ = writeln!(r, "Uncovered characters: {}", set.uncovered());

    let _ = writeln!(
        r,
        "\nKanji of a word with no KanjiVG entry: {}",
        set.uncovered_form_kanji.len()
    );
    for &c in &set.uncovered_form_kanji {
        let _ = writeln!(r, "  {}", describe(c));
    }
    let _ = writeln!(
        r,
        "\nDirect components with no KanjiVG entry: {}",
        set.uncovered_components.len()
    );
    for &c in &set.uncovered_components {
        let _ = writeln!(r, "  {}", describe(c));
    }

    let mut undeclared: Vec<_> = rows.entries.undeclared_tags().collect();
    undeclared.sort_by(|a, b| (a.category, &a.name).cmp(&(b.category, &b.name)));
    let _ = writeln!(
        r,
        "\nJMdict tags that the DTD does not declare: {}",
        undeclared.len()
    );
    for tag in undeclared {
        let _ = writeln!(r, "  {} {}", tag.category.as_str(), tag.name);
    }
    let skipped = &rows.entries.skipped_restrictions;
    let _ = writeln!(
        r,
        "\nRestrictions that name no form of their entry: {}",
        skipped.len()
    );
    for s in skipped {
        let _ = writeln!(r, "  ent_seq {}: <{}>{}", s.seq, s.element, s.text);
    }

    let _ = writeln!(
        r,
        "\nKanjiVG groups whose kvg:element value does not give exactly 1 code point after normalization: {}",
        findings.kvg_unresolved.len()
    );
    let mut unresolved = findings.kvg_unresolved.clone();
    unresolved.sort_by(|a, b| (a.kanji, &a.element).cmp(&(b.kanji, &b.element)));
    for group in &unresolved {
        let _ = writeln!(r, "  {}: {}", describe(group.kanji), group.element);
    }

    let _ = writeln!(r, "\nmutant rows: {}", set.mutants.len());
    for &(mutant, base) in &set.mutants {
        let _ = writeln!(r, "  {} → {}", describe(mutant), describe(base));
    }
    let _ = writeln!(
        r,
        "\nMutants in data/mutants.tsv that are not a KanjiVG component: {}",
        findings.not_kanjivg_components.len()
    );
    for &c in &findings.not_kanjivg_components {
        let _ = writeln!(r, "  {}", describe(c));
    }

    report_kanjidic(&mut r, findings);

    let _ = writeln!(
        r,
        "\nSize of content.db: {size} bytes ({:.1} MB)",
        size as f64 / 1e6
    );
    r
}

/// Adds the findings of stage 6 to the report `r`.
fn report_kanjidic(r: &mut String, findings: &Findings) {
    let set = &findings.set;
    let entries = &findings.kanjidic.entries;
    let _ = writeln!(r, "\nCharacters with a KANJIDIC2 entry: {}", entries.len());
    let _ = writeln!(
        r,
        "KANJIDIC2 entries that stage 6 ignores: {}",
        findings.kanjidic.ignored
    );

    let mutants: BTreeSet<char> = set.mutants.iter().map(|&(mutant, _)| mutant).collect();
    let no_entry = |c: &&char| !entries.contains_key(c);
    let lists: [(&str, Vec<char>); 3] = [
        (
            "Kanji of a word",
            findings
                .form_kanji
                .iter()
                .filter(no_entry)
                .copied()
                .collect(),
        ),
        (
            "Mutants",
            mutants
                .iter()
                .filter(|c| !findings.form_kanji.contains(c))
                .filter(no_entry)
                .copied()
                .collect(),
        ),
        (
            "Other components",
            set.characters
                .iter()
                .filter(|c| !findings.form_kanji.contains(c) && !mutants.contains(c))
                .filter(no_entry)
                .copied()
                .collect(),
        ),
    ];
    for (title, characters) in lists {
        let _ = writeln!(r, "\n{title} with no KANJIDIC2 entry: {}", characters.len());
        for c in characters {
            let _ = writeln!(r, "  {}", describe(c));
        }
    }

    let no_meaning: Vec<char> = entries
        .iter()
        .filter(|(_, entry)| entry.meanings.is_empty())
        .map(|(&c, _)| c)
        .collect();
    let _ = writeln!(
        r,
        "\nCharacters with a KANJIDIC2 entry and no English meaning: {}",
        no_meaning.len()
    );
    for c in no_meaning {
        let _ = writeln!(r, "  {}", describe(c));
    }
}

fn main() -> ExitCode {
    let result = parse_args().and_then(|args| {
        let report = run(&args)?;
        let path = args.out.join("build-report.txt");
        fs::write(&path, &report).with_context(|| format!("cannot write {}", path.display()))?;
        print!("{report}");
        Ok(())
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ingest: {error:#}");
            ExitCode::FAILURE
        }
    }
}
