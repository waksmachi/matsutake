//! Builds the sentence bundle of Tatoeba: the Japanese sentences of the Japanese index of Tatoeba,
//! with their furigana, their English translations, and the JMdict words that the index links them
//! to. The index comes from the Tanaka Corpus. `Docs/bundle-design.md` describes the bundle.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use jpdag::dictionary::{furigana_parts, word_rank};
use jpdag::kana::{is_kana, to_hiragana};
use jpdag::schema::{BUNDLE_APPLICATION_ID, BUNDLE_SCHEMA_VERSION, CREATE_BUNDLE_TABLES};
use rusqlite::{Connection, OpenFlags, params};

const USAGE: &str = "usage: tatoeba --content content.db --tatoeba sources/tatoeba-YYYY-MM-DD \
--out DIR";

const BUNDLE_ID: &str = "tatoeba";
const TITLE: &str = "Tatoeba: the sentences of the Japanese index";
const LICENCE: &str = "CC BY 2.0 FR";
const ATTRIBUTION: &str = "Sentences, translations, and furigana from Tatoeba \
(https://tatoeba.org), by their authors, under the Creative Commons Attribution 2.0 France \
licence. The Japanese index comes from the Tanaka Corpus of Yasuhito Tanaka, which the WWWJDIC \
project of Jim Breen corrected and indexed.";
const URL: &str = "https://tatoeba.org";
const SENTENCE_URL: &str = "https://tatoeba.org/sentences/show/{id}";

/// A word of a line of the index: `headword(reading)[sense]{form}~`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexWord {
    headword: String,
    /// A reading, or an entry id after `#`.
    reading: Option<String>,
    sense: Option<u32>,
    /// The word as the sentence writes it, if it is not the headword.
    form: Option<String>,
    checked: bool,
}

/// Reads a word of the index, or `None` for a word in another format.
fn parse_word(word: &str) -> Option<IndexWord> {
    let (rest, checked) = match word.strip_suffix('~') {
        Some(rest) => (rest, true),
        None => (word, false),
    };
    let (rest, form) = match rest.strip_suffix('}') {
        Some(rest) => {
            let (rest, form) = rest.rsplit_once('{')?;
            (rest, Some(form.to_owned()))
        }
        None => (rest, None),
    };
    let (rest, sense) = match rest.strip_suffix(']') {
        Some(rest) => {
            let (rest, sense) = rest.rsplit_once('[')?;
            (rest, Some(sense.parse().ok()?))
        }
        None => (rest, None),
    };
    let (headword, reading) = match rest.strip_suffix(')') {
        Some(rest) => {
            let (headword, reading) = rest.split_once('(')?;
            (headword, Some(reading.to_owned()))
        }
        None => (rest, None),
    };
    if headword.is_empty() || headword.contains(['(', '[', '{', '~']) {
        return None;
    }
    Some(IndexWord {
        headword: headword.to_owned(),
        reading,
        sense,
        form,
        checked,
    })
}

/// The JMdict entries of `content.db`, to find the entry of a word of the index.
#[derive(Default)]
struct Entries {
    ids: HashSet<u32>,
    by_form: HashMap<String, Vec<u32>>,
    by_reading: HashMap<String, Vec<u32>>,
    forms: HashMap<u32, Vec<String>>,
    readings: HashMap<u32, Vec<String>>,
    /// The frequency rank of each word, and of each reading in kana.
    rank: HashMap<u32, i64>,
    kana_rank: HashMap<(u32, String), i64>,
    senses: HashMap<u32, u32>,
}

/// How a word of the index found its entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rule {
    /// The index gives the entry id, or the headword names 1 entry.
    Direct,
    /// Of the entries, 1 has the headword as its first written form, or as its first reading.
    FirstForm,
    /// Of the entries, 1 is the most frequent.
    MostFrequent,
}

impl Entries {
    fn load(path: &Path) -> Result<Entries> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("opening {}", path.display()))?;
        let mut entries = Entries::default();
        let mut forms = conn.prepare("SELECT word_id, text FROM written_form ORDER BY position")?;
        for row in forms.query_map([], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)))? {
            let (id, text) = row?;
            entries.by_form.entry(text.clone()).or_default().push(id);
            entries.forms.entry(id).or_default().push(text);
        }
        let mut readings = conn.prepare("SELECT word_id, text FROM reading ORDER BY position")?;
        for row in readings.query_map([], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)))? {
            let (id, text) = row?;
            entries.by_reading.entry(text.clone()).or_default().push(id);
            entries.readings.entry(id).or_default().push(text);
        }
        let mut words = conn.prepare(&format!("SELECT id, {} FROM word", word_rank("word.id")))?;
        for row in words.query_map([], |r| {
            Ok((r.get::<_, u32>(0)?, r.get::<_, Option<i64>>(1)?))
        })? {
            let (id, rank) = row?;
            entries.ids.insert(id);
            if let Some(rank) = rank {
                entries.rank.insert(id, rank);
            }
        }
        let mut kana = conn.prepare(
            "SELECT r.word_id, r.text, f.rank FROM reading r
             JOIN reading_frequency f ON f.reading_id = r.id",
        )?;
        for row in kana.query_map([], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })? {
            let (id, text, rank) = row?;
            entries.kana_rank.insert((id, text), rank);
        }
        let mut senses = conn.prepare("SELECT word_id, count(*) FROM sense GROUP BY word_id")?;
        for row in senses.query_map([], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?)))? {
            let (id, count) = row?;
            entries.senses.insert(id, count);
        }
        Ok(entries)
    }

    /// The entry of a word of the index, with the rule that found it, or `None` when no entry or
    /// several entries fit.
    ///
    /// The index names a word by its headword, with a reading or an entry id when the headword
    /// alone is not enough. A kana headword names a word that JMdict writes in kana, if there is
    /// one: は is the particle, not 葉. When several entries fit, the one whose first written form
    /// (or first reading, for a word in kana) is the headword wins, and then the most frequent.
    fn resolve(&self, word: &IndexWord) -> Option<(u32, Rule)> {
        let head = &word.headword;
        if let Some(id) = word.reading.as_deref().and_then(|r| r.strip_prefix('#')) {
            let id: u32 = id.parse().ok()?;
            return self.ids.contains(&id).then_some((id, Rule::Direct));
        }
        let kana_head = head.chars().all(is_kana);
        let mut candidates: Vec<u32> = if kana_head && !self.by_form.contains_key(head) {
            let all = self.by_reading.get(head).cloned().unwrap_or_default();
            let kana_only: Vec<u32> = all
                .iter()
                .copied()
                .filter(|id| !self.forms.contains_key(id))
                .collect();
            if kana_only.is_empty() { all } else { kana_only }
        } else {
            self.by_form
                .get(head)
                .or_else(|| self.by_reading.get(head))
                .cloned()
                .unwrap_or_default()
        };
        candidates.sort_unstable();
        candidates.dedup();
        let reading = word.reading.as_deref().map(to_hiragana);
        if let Some(reading) = &reading {
            candidates.retain(|id| self.reads(*id, reading));
        }
        match candidates.len() {
            0 => return None,
            1 => return Some((candidates[0], Rule::Direct)),
            _ => {}
        }
        let mut first: Vec<u32> = candidates
            .iter()
            .copied()
            .filter(|id| match self.forms.get(id) {
                Some(forms) => forms.first() == Some(head),
                None => self.readings.get(id).and_then(|r| r.first()) == Some(head),
            })
            .collect();
        if let Some(reading) = &reading {
            let first_read: Vec<u32> = first
                .iter()
                .copied()
                .filter(|id| {
                    self.readings
                        .get(id)
                        .and_then(|r| r.first())
                        .is_some_and(|r| to_hiragana(r) == *reading)
                })
                .collect();
            if !first_read.is_empty() {
                first = first_read;
            }
        }
        if first.len() == 1 {
            return Some((first[0], Rule::FirstForm));
        }
        let pool = if first.is_empty() {
            &candidates
        } else {
            &first
        };
        // A kana headword is ranked by the better of its word and of that reading in kana, which
        // the rank of the word leaves out when other words share it: の is the particle (rank 1 in
        // kana), and くらい is 位 (word rank 338), not 暗い.
        let key = |id: &u32| {
            let rank = self.rank.get(id).copied().unwrap_or(i64::MAX);
            if kana_head {
                let kana = self.kana_rank.get(&(*id, head.clone())).copied();
                rank.min(kana.unwrap_or(i64::MAX))
            } else {
                rank
            }
        };
        let mut ranked = pool.clone();
        ranked.sort_by_key(key);
        (key(&ranked[0]) < key(&ranked[1]) && key(&ranked[0]) != i64::MAX)
            .then_some((ranked[0], Rule::MostFrequent))
    }

    /// Whether the entry `id` has a reading that is `hiragana` in hiragana.
    fn reads(&self, id: u32, hiragana: &str) -> bool {
        self.readings
            .get(&id)
            .is_some_and(|readings| readings.iter().any(|r| to_hiragana(r) == hiragana))
    }
}

/// Finds `form` in `text` at or after the character `from`, or else anywhere: its start and end in
/// characters.
fn find(text: &str, form: &str, from: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let form: Vec<char> = form.chars().collect();
    if form.is_empty() || form.len() > chars.len() {
        return None;
    }
    let at = |start: usize| chars[start..].starts_with(&form);
    let last = chars.len() - form.len();
    (from.min(last + 1)..=last)
        .chain(0..from.min(last + 1))
        .find(|&start| at(start))
        .map(|start| (start, start + form.len()))
}

/// Reads the lines of a TSV file of Tatoeba, and gives the fields of each.
fn lines(path: &Path, mut each: impl FnMut(Vec<&str>) -> Result<()>) -> Result<()> {
    let file = fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    for (number, line) in BufReader::new(file).lines().enumerate() {
        let line = line.with_context(|| format!("line {} of {}", number + 1, path.display()))?;
        each(line.split('\t').collect())
            .with_context(|| format!("line {} of {}", number + 1, path.display()))?;
    }
    Ok(())
}

fn author(name: &str) -> Option<String> {
    (!name.is_empty() && name != "\\N").then(|| name.to_owned())
}

struct Args {
    content: PathBuf,
    tatoeba: PathBuf,
    out: PathBuf,
}

fn parse_args() -> Result<Args> {
    let mut args = std::env::args_os().skip(1);
    let (mut content, mut tatoeba, mut out) = (None, None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.to_str() {
            Some("--content") => &mut content,
            Some("--tatoeba") => &mut tatoeba,
            Some("--out") => &mut out,
            _ => bail!("unknown argument {flag:?}\n{USAGE}"),
        };
        *slot = Some(PathBuf::from(
            args.next()
                .with_context(|| format!("{flag:?} needs a value"))?,
        ));
    }
    let need = |value: Option<PathBuf>, flag: &str| {
        value.with_context(|| format!("{flag} is missing\n{USAGE}"))
    };
    Ok(Args {
        content: need(content, "--content")?,
        tatoeba: need(tatoeba, "--tatoeba")?,
        out: need(out, "--out")?,
    })
}

/// The counts of the report of a build.
#[derive(Default)]
struct Counts {
    index_lines: usize,
    deleted: usize,
    sentences: usize,
    no_word: usize,
    words: usize,
    unparsed: usize,
    rules: HashMap<&'static str, usize>,
    unresolved: usize,
    senses_dropped: usize,
    forms_not_found: usize,
    checked: usize,
    furigana: usize,
    furigana_by_people: usize,
    furigana_mismatch: usize,
    untranslated: usize,
    translations: usize,
}

fn run(args: &Args) -> Result<()> {
    let version = args
        .tatoeba
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_prefix("tatoeba-"))
        .context("--tatoeba must be a folder named tatoeba-YYYY-MM-DD, as fetch-tatoeba.sh makes")?
        .to_owned();
    let entries = Entries::load(&args.content)?;
    let dir = &args.tatoeba;

    let mut japanese: HashMap<i64, (String, Option<String>)> = HashMap::new();
    lines(&dir.join("jpn_sentences_detailed.tsv"), |f| {
        if f.len() >= 4 {
            japanese.insert(f[0].parse()?, (f[2].to_owned(), author(f[3])));
        }
        Ok(())
    })?;
    let mut index: Vec<(i64, String)> = Vec::new();
    lines(&dir.join("jpn_indices.csv"), |f| {
        if f.len() >= 3 {
            index.push((f[0].parse()?, f[2].to_owned()));
        }
        Ok(())
    })?;
    index.sort_by_key(|(id, _)| *id);
    index.dedup_by_key(|(id, _)| *id);
    let indexed: HashSet<i64> = index.iter().map(|(id, _)| *id).collect();

    // The English translations of each sentence of the index, the oldest first: most often the
    // English of the Tanaka Corpus.
    let mut links: HashMap<i64, Vec<i64>> = HashMap::new();
    lines(&dir.join("jpn-eng_links.tsv"), |f| {
        if f.len() >= 2 {
            let (jpn, eng): (i64, i64) = (f[0].parse()?, f[1].parse()?);
            if indexed.contains(&jpn) {
                links.entry(jpn).or_default().push(eng);
            }
        }
        Ok(())
    })?;
    let wanted: HashSet<i64> = links.values().flatten().copied().collect();
    let mut english: HashMap<i64, (String, Option<String>)> = HashMap::new();
    lines(&dir.join("eng_sentences_detailed.tsv"), |f| {
        if f.len() >= 4 {
            let id: i64 = f[0].parse()?;
            if wanted.contains(&id) {
                english.insert(id, (f[2].to_owned(), author(f[3])));
            }
        }
        Ok(())
    })?;
    let translations: HashMap<i64, Vec<&(String, Option<String>)>> = links
        .into_iter()
        .map(|(jpn, mut engs)| {
            engs.sort();
            engs.dedup();
            (jpn, engs.iter().filter_map(|eng| english.get(eng)).collect::<Vec<_>>())
        })
        .filter(|(_, found)| !found.is_empty())
        .collect();

    let out_dir = &args.out;
    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    let path = out_dir.join("tatoeba.db");
    let partial = out_dir.join("tatoeba.db.tmp");
    let _ = fs::remove_file(&partial);
    let mut conn = Connection::open(&partial)?;
    conn.execute_batch(CREATE_BUNDLE_TABLES)?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO bundle VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            BUNDLE_ID,
            TITLE,
            version,
            LICENCE,
            ATTRIBUTION,
            URL,
            SENTENCE_URL
        ],
    )?;

    let mut counts = Counts {
        index_lines: index.len(),
        ..Counts::default()
    };
    // The furigana of each sentence, if they spell the sentence, and whether a person wrote them.
    let mut furigana: HashMap<i64, (String, bool)> = HashMap::new();
    lines(&dir.join("jpn_transcriptions.tsv"), |f| {
        if f.len() < 5 || f[2] != "Hrkt" {
            return Ok(());
        }
        let id: i64 = f[0].parse()?;
        let Some((text, _)) = japanese.get(&id) else {
            return Ok(());
        };
        let spells = furigana_parts(f[4]).is_some_and(|parts| {
            parts.iter().map(|p| p.text.as_str()).collect::<String>() == *text
        });
        if spells {
            furigana.insert(id, (f[4].to_owned(), author(f[3]).is_some()));
        } else {
            // The furigana are in another format, or the sentence changed after them.
            counts.furigana_mismatch += 1;
        }
        Ok(())
    })?;

    {
        let mut sentence = tx.prepare("INSERT INTO sentence VALUES (?1, ?2, ?3, ?4)")?;
        let mut translation =
            tx.prepare("INSERT INTO translation VALUES (?1, ?2, 'eng', ?3, ?4)")?;
        let mut sentence_word =
            tx.prepare("INSERT INTO sentence_word VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
        for (id, line) in &index {
            let Some((text, owner)) = japanese.get(id) else {
                counts.deleted += 1;
                continue;
            };
            // A sentence is an example for a learner only with its English.
            let Some(english) = translations.get(id) else {
                counts.untranslated += 1;
                continue;
            };
            let mut rows = Vec::new();
            let mut cursor = 0;
            for raw in line.split_whitespace() {
                counts.words += 1;
                let Some(word) = parse_word(raw) else {
                    counts.unparsed += 1;
                    continue;
                };
                let Some((word_id, rule)) = entries.resolve(&word) else {
                    counts.unresolved += 1;
                    continue;
                };
                *counts
                    .rules
                    .entry(match rule {
                        Rule::Direct => "direct",
                        Rule::FirstForm => "first form",
                        Rule::MostFrequent => "most frequent",
                    })
                    .or_default() += 1;
                let sense = word.sense.filter(|&s| {
                    let fits = s >= 1 && s <= entries.senses.get(&word_id).copied().unwrap_or(0);
                    if !fits {
                        counts.senses_dropped += 1;
                    }
                    fits
                });
                let form = word.form.as_deref().unwrap_or(&word.headword);
                let span = find(text, form, cursor);
                match span {
                    Some((_, end)) => cursor = end,
                    None => counts.forms_not_found += 1,
                }
                counts.checked += usize::from(word.checked);
                rows.push((word_id, sense, span, word.checked));
            }
            if rows.is_empty() {
                counts.no_word += 1;
                continue;
            }
            let (furigana, by_person) = match furigana.get(id) {
                Some((text, by_person)) => (Some(text.as_str()), *by_person),
                None => (None, false),
            };
            counts.furigana += usize::from(furigana.is_some());
            counts.furigana_by_people += usize::from(by_person);
            sentence.execute(params![id, text, furigana, owner])?;
            for (position, (text, owner)) in (1..).zip(english) {
                translation.execute(params![id, position, text, owner])?;
                counts.translations += 1;
            }
            for (position, (word_id, sense, span, checked)) in rows.into_iter().enumerate() {
                sentence_word.execute(params![
                    word_id,
                    id,
                    position as i64 + 1,
                    sense,
                    span.map(|s| s.0 as i64),
                    span.map(|s| s.1 as i64),
                    checked,
                ])?;
            }
            counts.sentences += 1;
        }
    }

    tx.commit()?;
    conn.pragma_update(None, "user_version", BUNDLE_SCHEMA_VERSION)?;
    conn.pragma_update(None, "application_id", BUNDLE_APPLICATION_ID)?;
    conn.execute_batch("ANALYZE; VACUUM;")?;
    drop(conn);
    fs::rename(&partial, &path).with_context(|| format!("writing {}", path.display()))?;

    let report = report(&version, &counts);
    let report_path = out_dir.join("tatoeba-report.txt");
    fs::write(&report_path, &report)
        .with_context(|| format!("writing {}", report_path.display()))?;
    print!("{report}");
    eprintln!("wrote {} and {}", path.display(), report_path.display());
    Ok(())
}

fn report(version: &str, c: &Counts) -> String {
    let percent = |part: usize, whole: usize| 100.0 * part as f64 / whole.max(1) as f64;
    let resolved: usize = c.rules.values().sum();
    let mut out = String::new();
    let _ = writeln!(out, "Tatoeba bundle, export of {version}\n");
    let _ = writeln!(
        out,
        "Lines of the Japanese index        {:>9}",
        c.index_lines
    );
    let _ = writeln!(out, "  for deleted sentences            {:>9}", c.deleted);
    let _ = writeln!(out, "  with no English translation      {:>9}", c.untranslated);
    let _ = writeln!(out, "  with no word that resolves       {:>9}", c.no_word);
    let _ = writeln!(out, "Sentences in the bundle            {:>9}", c.sentences);
    let _ = writeln!(out, "  English translations             {:>9}", c.translations);
    let _ = writeln!(
        out,
        "  with furigana                    {:>9} ({:.1}%), {} by a person",
        c.furigana,
        percent(c.furigana, c.sentences),
        c.furigana_by_people
    );
    let _ = writeln!(
        out,
        "Furigana that do not spell their sentence {:>3}",
        c.furigana_mismatch
    );
    let _ = writeln!(out, "Words of the index                 {:>9}", c.words);
    let _ = writeln!(out, "  in another format                {:>9}", c.unparsed);
    let _ = writeln!(
        out,
        "  with no entry, or several        {:>9} ({:.2}%)",
        c.unresolved,
        percent(c.unresolved, c.words)
    );
    let _ = writeln!(out, "  linked to an entry               {:>9}", resolved);
    for rule in ["direct", "first form", "most frequent"] {
        let n = c.rules.get(rule).copied().unwrap_or(0);
        let _ = writeln!(
            out,
            "    by {rule:<28}{n:>9} ({:.2}%)",
            percent(n, resolved)
        );
    }
    let _ = writeln!(out, "  marked as good examples (~)      {:>9}", c.checked);
    let _ = writeln!(
        out,
        "  sense numbers past the last sense {:>8}",
        c.senses_dropped
    );
    let _ = writeln!(
        out,
        "  forms not found in the sentence  {:>9}",
        c.forms_not_found
    );
    out
}

fn main() -> ExitCode {
    match parse_args().and_then(|args| run(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tatoeba: {error:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_words_give_their_parts() {
        assert_eq!(
            parse_word("為る(する)[02]{した}~").unwrap(),
            IndexWord {
                headword: "為る".into(),
                reading: Some("する".into()),
                sense: Some(2),
                form: Some("した".into()),
                checked: true,
            }
        );
        let particle = parse_word("が(#2028930)").unwrap();
        assert_eq!(particle.reading.as_deref(), Some("#2028930"));
        assert_eq!(parse_word("昼寝").unwrap().form, None);
        assert_eq!(parse_word("{した}"), None);
    }

    #[test]
    fn forms_are_found_after_the_word_before() {
        assert_eq!(find("猫と猫", "猫", 1), Some((2, 3)));
        assert_eq!(find("猫と犬", "猫", 2), Some((0, 1)));
        assert_eq!(find("猫と犬", "鳥", 0), None);
    }

    fn entries() -> Entries {
        let mut e = Entries::default();
        let mut add = |id: u32, forms: &[&str], readings: &[&str], rank: Option<i64>| {
            e.ids.insert(id);
            for f in forms {
                e.by_form.entry((*f).into()).or_default().push(id);
                e.forms.entry(id).or_default().push((*f).into());
            }
            for r in readings {
                e.by_reading.entry((*r).into()).or_default().push(id);
                e.readings.entry(id).or_default().push((*r).into());
            }
            if let Some(rank) = rank {
                e.rank.insert(id, rank);
            }
        };
        add(1, &[], &["は"], Some(2));
        add(2, &["葉"], &["は"], Some(9000));
        add(3, &["乃", "之"], &["の"], Some(12868));
        add(4, &["野"], &["の"], Some(8032));
        add(5, &["為る"], &["する"], Some(10));
        add(6, &["為る"], &["なる"], Some(400));
        e.kana_rank.insert((3, "の".into()), 1);
        e
    }

    fn resolve(e: &Entries, word: &str) -> Option<(u32, Rule)> {
        e.resolve(&parse_word(word).unwrap())
    }

    #[test]
    fn a_kana_headword_names_a_word_in_kana() {
        assert_eq!(resolve(&entries(), "は"), Some((1, Rule::Direct)));
    }

    #[test]
    fn a_kana_headword_can_use_its_rank_in_kana() {
        // の is the particle, rank 1 in kana, though 野 has the better word rank.
        assert_eq!(resolve(&entries(), "の"), Some((3, Rule::MostFrequent)));
    }

    #[test]
    fn a_reading_or_an_id_picks_the_entry() {
        let e = entries();
        assert_eq!(resolve(&e, "為る(する)"), Some((5, Rule::Direct)));
        assert_eq!(resolve(&e, "為る(なる)"), Some((6, Rule::Direct)));
        assert_eq!(resolve(&e, "が(#2)"), Some((2, Rule::Direct)));
        assert_eq!(resolve(&e, "が(#99)"), None);
    }
}
