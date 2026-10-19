//! Stage 9: read the frequency ranks of Jiten's drama list. Give each written form the rank of
//! its word written in that form, and each reading the rank of its word written in kana.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::jmdict::Category;
use crate::words::EntryRows;

/// The mark of Jiten on the rank of a word written in kana.
const KANA_MARK: char = '㋕';

/// The rows of stage 9, and what the build report says about the file.
#[derive(Debug, Default)]
pub struct Frequency {
    /// Date of the list, from its revision, for example 2026-10-09.
    pub version: Option<String>,
    /// Number of entries of the list.
    pub entries: usize,
    /// Number of entries that are no written form and reading of a word, and no reading.
    pub entries_without_word: usize,
    /// Number of entries that can be 2 or more words, and that go to no word.
    pub ambiguous_entries: usize,
    /// Number of entries that can be 2 or more words, and that go to the only 1 of them that
    /// JMdict marks as common.
    pub ties_broken: usize,
    /// `written_form_frequency` rows, each as (written_form_id, rank).
    pub form_rows: Vec<(u32, u32)>,
    /// `reading_frequency` rows, each as (reading_id, rank).
    pub reading_rows: Vec<(u32, u32)>,
}

impl Frequency {
    /// The best rank of each word with a rank: of the word in 1 of its written forms, or in kana.
    /// A rank in kana counts for a word that JMdict writes in kana (a word with no written form,
    /// or a word whose first sense has the tag `uk`), and for a reading that no other word has.
    /// Jiten counts a reading in kana for the words of other words with the same sound, as
    /// `jpdag::dictionary` says.
    pub fn word_ranks(&self, entries: &EntryRows) -> HashMap<u32, u32> {
        let in_kana = words_in_kana(entries);
        let mut words_of: HashMap<&str, HashSet<u32>> = HashMap::new();
        for reading in &entries.readings {
            words_of
                .entry(&reading.text)
                .or_default()
                .insert(reading.word_id);
        }
        let form_words: HashMap<u32, u32> = entries
            .written_forms
            .iter()
            .map(|f| (f.id, f.word_id))
            .collect();
        let reading_words: HashMap<u32, u32> = entries
            .readings
            .iter()
            .filter(|r| in_kana.contains(&r.word_id) || words_of[r.text.as_str()].len() == 1)
            .map(|r| (r.id, r.word_id))
            .collect();
        let forms = self
            .form_rows
            .iter()
            .map(|&(id, rank)| (form_words.get(&id), rank));
        let readings = self
            .reading_rows
            .iter()
            .map(|&(id, rank)| (reading_words.get(&id), rank));
        let mut ranks = HashMap::new();
        for (word_id, rank) in forms.chain(readings) {
            if let Some(&word_id) = word_id {
                keep_best(&mut ranks, word_id, rank);
            }
        }
        ranks
    }
}

/// The words that JMdict writes in kana: the words with no written form, and the words whose first
/// sense has the tag `uk`.
fn words_in_kana(entries: &EntryRows) -> HashSet<u32> {
    let uk: HashSet<u32> = entries
        .tags
        .iter()
        .filter(|tag| tag.category == Category::Misc && tag.name == "uk")
        .map(|tag| tag.id)
        .collect();
    let first_senses: HashMap<u32, u32> = entries
        .senses
        .iter()
        .filter(|sense| sense.position == 1)
        .map(|sense| (sense.id, sense.word_id))
        .collect();
    let with_forms: HashSet<u32> = entries.written_forms.iter().map(|f| f.word_id).collect();
    let mut words: HashSet<u32> = entries
        .words
        .iter()
        .copied()
        .filter(|word| !with_forms.contains(word))
        .collect();
    words.extend(
        entries
            .sense_tags
            .iter()
            .filter(|(_, tag_id)| uk.contains(tag_id))
            .filter_map(|(sense_id, _)| first_senses.get(sense_id)),
    );
    words
}

/// The `index.json` of a Yomitan dictionary.
#[derive(Deserialize)]
struct Index {
    revision: Option<String>,
}

/// The data of a `freq` entry: a rank alone, or a reading with a rank.
#[derive(Deserialize)]
#[serde(untagged)]
enum Data {
    Paired { reading: String, frequency: Rank },
    Alone(Rank),
}

#[derive(Deserialize)]
struct Rank {
    value: u32,
    #[serde(rename = "displayValue")]
    display_value: String,
}

/// 1 entry of a `term_meta_bank`: the term, the mode, and the data.
type Entry = (String, String, serde_json::Value);

/// A written form and a reading of a word, as (word_id, written_form_id, reading_id).
type Pair = (u32, u32, u32);

/// Gives the date of a revision such as `Jiten (Drama) 26-10-09`: 2026-10-09. A revision with no
/// date at its end is the version as it is.
fn version_from_revision(revision: &str) -> String {
    let last = revision.rsplit(' ').next().unwrap_or(revision);
    let parts: Vec<&str> = last.split('-').collect();
    let is_date = parts.len() == 3
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit()));
    if is_date {
        format!("20{last}")
    } else {
        revision.to_owned()
    }
}

/// Reads the Yomitan dictionary of Jiten at `path`, and gives the ranks of the words.
pub fn read(path: &Path, entries: &EntryRows) -> Result<Frequency> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let (index, banks) =
        read_zip(file).with_context(|| format!("cannot read {}", path.display()))?;
    rank(&index, &banks, entries)
}

/// Gives the `index.json` and each `term_meta_bank_N.json` of a Yomitan dictionary, the banks in
/// the order of N.
fn read_zip(reader: impl Read + Seek) -> Result<(String, Vec<String>)> {
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut index = None;
    let mut banks = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name()?.into_owned();
        let number = name
            .strip_prefix("term_meta_bank_")
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|n| n.parse::<u32>().ok());
        if name == "index.json" || number.is_some() {
            let mut text = String::new();
            file.read_to_string(&mut text)?;
            match number {
                Some(n) => banks.push((n, text)),
                None => index = Some(text),
            }
        }
    }
    banks.sort_by_key(|&(n, _)| n);
    let index = index.context("the dictionary has no index.json")?;
    Ok((index, banks.into_iter().map(|(_, text)| text).collect()))
}

/// Gives the ranks of the entries of the banks to the written forms and readings of `entries`.
///
/// An entry with a reading is a written form and a reading: the word with that written form and
/// that reading. Its rank is the rank of the written form, or with the mark ㋕, the rank of the
/// reading written in kana. An entry with no reading is a reading written in kana. Jiten gives it
/// to the first word that has it, and the entries with a reading already give the rank of that
/// word, unless the word is in kana alone. Jiten takes the words from the most frequent, so an
/// entry that can be 2 or more words goes to the only 1 that JMdict marks as common, if there is
/// one.
fn rank(index: &str, banks: &[String], entries: &EntryRows) -> Result<Frequency> {
    let index: Index = serde_json::from_str(index).context("bad index.json")?;
    let mut frequency = Frequency {
        version: index.revision.as_deref().map(version_from_revision),
        ..Frequency::default()
    };

    // Each written form and reading of a word, by their texts.
    let mut readings_of: HashMap<u32, Vec<(u32, &str)>> = HashMap::new();
    for reading in &entries.readings {
        readings_of
            .entry(reading.word_id)
            .or_default()
            .push((reading.id, &reading.text));
    }
    let mut pairs: HashMap<(&str, &str), Vec<Pair>> = HashMap::new();
    let mut with_forms = HashSet::new();
    for form in &entries.written_forms {
        with_forms.insert(form.word_id);
        for &(reading_id, reading) in readings_of.get(&form.word_id).into_iter().flatten() {
            pairs.entry((&form.text, reading)).or_default().push((
                form.word_id,
                form.id,
                reading_id,
            ));
        }
    }
    // Each reading, by its text, as (word_id, reading_id).
    let mut kana: HashMap<&str, Vec<(u32, u32)>> = HashMap::new();
    for reading in &entries.readings {
        kana.entry(&reading.text)
            .or_default()
            .push((reading.word_id, reading.id));
    }

    let common_forms: HashSet<u32> = entries.common_written_forms.iter().copied().collect();
    let common_readings: HashSet<u32> = entries.common_readings.iter().copied().collect();
    let mut form_ranks: HashMap<u32, u32> = HashMap::new();
    let mut reading_ranks: HashMap<u32, u32> = HashMap::new();
    let mut alone = Vec::new();
    for (number, bank) in banks.iter().enumerate() {
        let list: Vec<Entry> = serde_json::from_str(bank)
            .with_context(|| format!("bad term_meta_bank_{}.json", number + 1))?;
        for (term, mode, data) in list {
            if mode != "freq" {
                continue;
            }
            frequency.entries += 1;
            let data: Data = serde_json::from_value(data)
                .with_context(|| format!("bad frequency of {term:?}"))?;
            let (reading, rank) = match data {
                Data::Paired { reading, frequency } => (reading, frequency),
                Data::Alone(rank) => {
                    alone.push((term, rank.value));
                    continue;
                }
            };
            let in_kana = rank.display_value.ends_with(KANA_MARK);
            let matches = pairs
                .get(&(term.as_str(), reading.as_str()))
                .map_or(&[][..], Vec::as_slice);
            // A rank in kana is the rank of the reading, and the other rank is the rank of the
            // written form, so a tie goes to the word whose reading, or written form, is common.
            let is_common = |&&(_, form_id, reading_id): &&Pair| {
                if in_kana {
                    common_readings.contains(&reading_id)
                } else {
                    common_forms.contains(&form_id)
                }
            };
            let Some(&(_, form_id, reading_id)) = choose(matches, is_common, &mut frequency) else {
                continue;
            };
            if in_kana {
                keep_best(&mut reading_ranks, reading_id, rank.value);
            } else {
                keep_best(&mut form_ranks, form_id, rank.value);
            }
        }
    }

    // A reading in kana that an entry with a reading already ranks belongs to that word. The other
    // readings go to the words in kana alone with the reading, or if there are none, to the words
    // with it: to the only word, or to the only 1 whose reading JMdict marks as common.
    let ranked: HashSet<(&str, u32)> = entries
        .readings
        .iter()
        .filter_map(|r| {
            reading_ranks
                .get(&r.id)
                .map(|&rank| (r.text.as_str(), rank))
        })
        .collect();
    for (term, rank) in alone {
        if ranked.contains(&(term.as_str(), rank)) {
            continue;
        }
        let candidates = kana.get(term.as_str()).map_or(&[][..], Vec::as_slice);
        let kana_alone: Vec<(u32, u32, u32)> = candidates
            .iter()
            .filter(|(word_id, _)| !with_forms.contains(word_id))
            .map(|&(word_id, reading_id)| (word_id, 0, reading_id))
            .collect();
        let pool: Vec<(u32, u32, u32)> = if kana_alone.is_empty() {
            candidates
                .iter()
                .map(|&(word_id, reading_id)| (word_id, 0, reading_id))
                .collect()
        } else {
            kana_alone
        };
        let is_common = |&&(_, _, reading_id): &&Pair| common_readings.contains(&reading_id);
        if let Some(&(_, _, reading_id)) = choose(&pool, is_common, &mut frequency) {
            keep_best(&mut reading_ranks, reading_id, rank);
        }
    }

    frequency.form_rows = sorted(form_ranks);
    frequency.reading_rows = sorted(reading_ranks);
    if frequency.entries == 0 {
        bail!("the dictionary has no frequency entries");
    }
    Ok(frequency)
}

/// Chooses the word of an entry from the written forms and readings that match it: the only word,
/// or the only common word of 2 or more. Counts an entry of no word, or of 2 or more words and no
/// choice, in `frequency`.
fn choose<'a>(
    matches: &'a [Pair],
    is_common: impl Fn(&&Pair) -> bool,
    frequency: &mut Frequency,
) -> Option<&'a Pair> {
    let words: HashSet<u32> = matches.iter().map(|&(word_id, _, _)| word_id).collect();
    match words.len() {
        0 => {
            frequency.entries_without_word += 1;
            None
        }
        1 => matches.first(),
        _ => {
            let common: Vec<&Pair> = matches.iter().filter(&is_common).collect();
            let common_words: HashSet<u32> =
                common.iter().map(|&&(word_id, _, _)| word_id).collect();
            if common_words.len() == 1 {
                frequency.ties_broken += 1;
                common.first().copied()
            } else {
                frequency.ambiguous_entries += 1;
                None
            }
        }
    }
}

/// Keeps the best rank of `id`: the smallest.
fn keep_best(ranks: &mut HashMap<u32, u32>, id: u32, rank: u32) {
    ranks
        .entry(id)
        .and_modify(|best| *best = (*best).min(rank))
        .or_insert(rank);
}

fn sorted(ranks: HashMap<u32, u32>) -> Vec<(u32, u32)> {
    let mut rows: Vec<(u32, u32)> = ranks.into_iter().collect();
    rows.sort_unstable();
    rows
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;
    use crate::words::{ReadingRow, SenseRow, TagRow, WrittenFormRow};

    const INDEX: &str = r#"{"title":"Jiten (Drama)","revision":"Jiten (Drama) 26-10-09"}"#;

    /// The rows of words, each as (word_id, written forms, readings).
    fn entries(words: &[(u32, &[&str], &[&str])]) -> EntryRows {
        let mut rows = EntryRows::default();
        for &(word_id, forms, readings) in words {
            rows.words.push(word_id);
            for (i, &form) in forms.iter().enumerate() {
                rows.written_forms.push(WrittenFormRow {
                    id: rows.written_forms.len() as u32 + 1,
                    word_id,
                    position: i as u32 + 1,
                    text: form.into(),
                });
            }
            for (i, &reading) in readings.iter().enumerate() {
                rows.readings.push(ReadingRow {
                    id: rows.readings.len() as u32 + 1,
                    word_id,
                    position: i as u32 + 1,
                    text: reading.into(),
                    no_kanji: false,
                });
            }
        }
        rows
    }

    fn run(bank: &str, entries: &EntryRows) -> Frequency {
        rank(INDEX, &[bank.to_owned()], entries).unwrap()
    }

    #[test]
    fn plain_rank_goes_to_the_written_form() {
        let words = entries(&[(1, &["方"], &["ほう"]), (2, &["方"], &["かた"])]);
        let frequency = run(
            r#"[["方","freq",{"reading":"ほう","frequency":{"value":206,"displayValue":"206"}}],
                ["方","freq",{"reading":"かた","frequency":{"value":292,"displayValue":"292"}}]]"#,
            &words,
        );
        assert_eq!(frequency.form_rows, [(1, 206), (2, 292)]);
        assert!(frequency.reading_rows.is_empty());
        assert_eq!(frequency.version.as_deref(), Some("2026-10-09"));
    }

    #[test]
    fn kana_rank_goes_to_the_reading() {
        let words = entries(&[(1, &["一寸"], &["ちょっと", "ちょいと"])]);
        let frequency = run(
            r#"[["一寸","freq",{"reading":"ちょっと","frequency":{"value":46,"displayValue":"46㋕"}}],
                ["一寸","freq",{"reading":"ちょいと","frequency":{"value":6639,"displayValue":"6639㋕"}}],
                ["一寸","freq",{"reading":"ちょっと","frequency":{"value":47988,"displayValue":"47988"}}],
                ["一寸","freq",{"reading":"ちょいと","frequency":{"value":47988,"displayValue":"47988"}}]]"#,
            &words,
        );
        assert_eq!(frequency.reading_rows, [(1, 46), (2, 6639)]);
        // The written form has 1 rank, whatever the reading of the entry.
        assert_eq!(frequency.form_rows, [(1, 47988)]);
    }

    #[test]
    fn best_rank_wins() {
        let words = entries(&[(1, &["明日"], &["あした"])]);
        let frequency = run(
            r#"[["明日","freq",{"reading":"あした","frequency":{"value":300,"displayValue":"300"}}],
                ["明日","freq",{"reading":"あした","frequency":{"value":100,"displayValue":"100"}}]]"#,
            &words,
        );
        assert_eq!(frequency.form_rows, [(1, 100)]);
    }

    #[test]
    fn kana_alone_that_a_pair_ranks_is_skipped() {
        // ちょっと is a reading of 一寸 and of a word in kana alone. Jiten gave the kana to 一寸.
        let words = entries(&[(1, &["一寸"], &["ちょっと"]), (2, &[], &["ちょっと"])]);
        let frequency = run(
            r#"[["ちょっと","freq",{"value":46,"displayValue":"46㋕"}],
                ["一寸","freq",{"reading":"ちょっと","frequency":{"value":46,"displayValue":"46㋕"}}]]"#,
            &words,
        );
        assert_eq!(frequency.reading_rows, [(1, 46)]);
        assert_eq!(frequency.ambiguous_entries, 0);
    }

    #[test]
    fn kana_alone_goes_to_the_word_in_kana_alone() {
        let words = entries(&[(1, &["葉"], &["は"]), (2, &[], &["は"])]);
        let frequency = run(
            r#"[["は","freq",{"value":2,"displayValue":"2㋕"}]]"#,
            &words,
        );
        assert_eq!(frequency.reading_rows, [(2, 2)]);
    }

    #[test]
    fn kana_alone_goes_to_the_only_word_with_the_reading() {
        let words = entries(&[(1, &["休み"], &["やすみ"])]);
        let frequency = run(
            r#"[["やすみ","freq",{"value":80,"displayValue":"80㋕"}]]"#,
            &words,
        );
        assert_eq!(frequency.reading_rows, [(1, 80)]);
    }

    #[test]
    fn entries_of_2_words_are_counted_and_skipped() {
        let words = entries(&[
            (1, &["一寸"], &["ちょっと"]),
            (2, &["一寸"], &["ちょっと"]),
            (3, &[], &["はし"]),
            (4, &[], &["はし"]),
        ]);
        let frequency = run(
            r#"[["一寸","freq",{"reading":"ちょっと","frequency":{"value":9,"displayValue":"9"}}],
                ["はし","freq",{"value":36023,"displayValue":"36023㋕"}]]"#,
            &words,
        );
        assert!(frequency.form_rows.is_empty() && frequency.reading_rows.is_empty());
        assert_eq!(frequency.ambiguous_entries, 2);
    }

    #[test]
    fn kana_alone_of_2_words_goes_to_the_common_word() {
        // は is the particle (common) and "yes" (not common), both in kana alone.
        let mut words = entries(&[(1, &[], &["は"]), (2, &[], &["は"])]);
        words.common_readings.push(1);
        let frequency = run(
            r#"[["は","freq",{"value":2,"displayValue":"2㋕"}]]"#,
            &words,
        );
        assert_eq!(frequency.reading_rows, [(1, 2)]);
        assert_eq!(frequency.ties_broken, 1);
        assert_eq!(frequency.ambiguous_entries, 0);
    }

    #[test]
    fn pair_of_2_words_goes_to_the_word_with_the_common_form() {
        let mut words = entries(&[(1, &["一寸"], &["ちょっと"]), (2, &["一寸"], &["ちょっと"])]);
        words.common_written_forms.push(2);
        let frequency = run(
            r#"[["一寸","freq",{"reading":"ちょっと","frequency":{"value":9,"displayValue":"9"}}],
                ["一寸","freq",{"reading":"ちょっと","frequency":{"value":4,"displayValue":"4㋕"}}]]"#,
            &words,
        );
        // The written form of word 2 is common, but no reading is, so the rank in kana stays out.
        assert_eq!(frequency.form_rows, [(2, 9)]);
        assert!(frequency.reading_rows.is_empty());
        assert_eq!(frequency.ties_broken, 1);
        assert_eq!(frequency.ambiguous_entries, 1);
    }

    #[test]
    fn entries_of_no_word_are_counted() {
        let words = entries(&[(1, &["語る"], &["かたる"])]);
        let frequency = run(
            r#"[["語学","freq",{"reading":"ごがく","frequency":{"value":9,"displayValue":"9"}}],
                ["はずだ","freq",{"value":40,"displayValue":"40㋕"}]]"#,
            &words,
        );
        assert_eq!(frequency.entries, 2);
        assert_eq!(frequency.entries_without_word, 2);
    }

    #[test]
    fn word_rank_is_the_best_rank_of_its_forms_and_readings() {
        let mut words = entries(&[
            (1, &["一寸"], &["ちょっと"]),
            (2, &["休み"], &["やすみ"]),
            (3, &["慣れる"], &["なれる"]),
            (4, &["馴れる"], &["なれる"]),
            (5, &["綺麗"], &["きれい"]),
        ]);
        // 一寸 is usually written in kana: the first sense of word 1 has the tag uk.
        words.senses.push(SenseRow {
            id: 1,
            word_id: 1,
            position: 1,
            note: None,
        });
        words.tags.push(TagRow {
            id: 1,
            category: Category::Misc,
            name: "uk".into(),
            description: None,
        });
        words.sense_tags.push((1, 1));
        let frequency = run(
            r#"[["一寸","freq",{"reading":"ちょっと","frequency":{"value":47988,"displayValue":"47988"}}],
                ["一寸","freq",{"reading":"ちょっと","frequency":{"value":46,"displayValue":"46㋕"}}],
                ["休み","freq",{"reading":"やすみ","frequency":{"value":80,"displayValue":"80"}}],
                ["慣れる","freq",{"reading":"なれる","frequency":{"value":948,"displayValue":"948"}}],
                ["慣れる","freq",{"reading":"なれる","frequency":{"value":402,"displayValue":"402㋕"}}],
                ["綺麗","freq",{"reading":"きれい","frequency":{"value":7364,"displayValue":"7364"}}],
                ["綺麗","freq",{"reading":"きれい","frequency":{"value":675,"displayValue":"675㋕"}}]]"#,
            &words,
        );
        // The kana rank of 慣れる does not count: its sense has no tag uk, and 馴れる has its
        // reading. No other word reads きれい, so the kana rank of 綺麗 counts.
        assert_eq!(
            frequency.word_ranks(&words),
            HashMap::from([(1, 46), (2, 80), (3, 948), (5, 675)])
        );
    }

    #[test]
    fn version_comes_from_the_revision() {
        assert_eq!(
            version_from_revision("Jiten (Drama) 26-10-09"),
            "2026-10-09"
        );
        assert_eq!(version_from_revision("Jiten (Drama)"), "Jiten (Drama)");
    }

    #[test]
    fn reads_the_banks_of_a_zip_in_order() {
        let mut bytes = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut bytes));
            let options = zip::write::SimpleFileOptions::default();
            for (name, text) in [
                ("term_meta_bank_2.json", "[2]"),
                ("index.json", INDEX),
                ("term_meta_bank_1.json", "[1]"),
                ("styles.css", ""),
            ] {
                zip.start_file(name, options).unwrap();
                zip.write_all(text.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let (index, banks) = read_zip(Cursor::new(bytes)).unwrap();
        assert_eq!(index, INDEX);
        assert_eq!(banks, ["[1]", "[2]"]);
    }
}
