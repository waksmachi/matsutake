//! Stage 10: count the kanji of the Japanese sentences of Tatoeba, and rank them. Give each kanji
//! of the character set its rank and its count.

use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::{Context, Result};
use jpdag::normalize::normalize_str;

/// The file of the Japanese sentences, in a folder of `fetch-tatoeba.sh`.
const SENTENCES: &str = "jpn_sentences_detailed.tsv";

/// The rows of stage 10, and what the build report says about the sentences.
#[derive(Debug, Default)]
pub struct KanjiFrequency {
    /// Date of the export, from the name of its folder, for example 2026-10-17.
    pub version: Option<String>,
    /// Number of sentences.
    pub sentences: usize,
    /// Number of kanji in the sentences, each time that it occurs.
    pub occurrences: u64,
    /// Kanji of the sentences that are not in the character set.
    pub outside_set: BTreeSet<char>,
    /// `character_frequency` rows, each as (character_id, rank, count).
    pub rows: Vec<(u32, u32, u32)>,
}

/// Whether `c` is a kanji: a code point of the CJK Unified Ideographs blocks, after
/// normalization. 々 and 〆 stand for kanji, but are not kanji.
pub fn is_kanji(c: char) -> bool {
    matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{20000}'..='\u{3FFFF}')
}

/// The version of the export of the folder `dir`: tatoeba-2026-10-17 gives 2026-10-17.
pub fn version_from_path(dir: &Path) -> Option<String> {
    let name = dir.file_name()?.to_str()?;
    Some(name.strip_prefix("tatoeba-")?.to_owned())
}

/// Counts the kanji of the sentences of the export in `dir`, and ranks them: the most frequent is
/// rank 1, and kanji with the same count share the rank of the first of them. The rank is among
/// all the kanji of the sentences; the rows hold the kanji of `characters` only.
pub fn read(dir: &Path, characters: &BTreeSet<char>) -> Result<KanjiFrequency> {
    let path = dir.join(SENTENCES);
    let file = File::open(&path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut frequency = count(BufReader::new(file), characters)
        .with_context(|| format!("cannot read {}", path.display()))?;
    frequency.version = version_from_path(dir);
    Ok(frequency)
}

/// Counts the kanji of the sentences of `tsv`, as [`read`] does. Each line is the id, the
/// language, the text, and the other fields of a sentence, separated by tabs.
fn count(tsv: impl BufRead, characters: &BTreeSet<char>) -> Result<KanjiFrequency> {
    let mut frequency = KanjiFrequency::default();
    let mut counts: HashMap<char, u32> = HashMap::new();
    for line in tsv.lines() {
        let line = line?;
        let Some(text) = line.split('\t').nth(2) else {
            continue;
        };
        frequency.sentences += 1;
        for c in normalize_str(text).chars().filter(|&c| is_kanji(c)) {
            *counts.entry(c).or_default() += 1;
            frequency.occurrences += 1;
        }
    }
    let mut ranked: Vec<(char, u32)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut rank = 0;
    let mut previous = None;
    for (place, &(c, count)) in (1..).zip(&ranked) {
        if previous != Some(count) {
            rank = place;
            previous = Some(count);
        }
        if characters.contains(&c) {
            frequency.rows.push((c as u32, rank, count));
        } else {
            frequency.outside_set.insert(c);
        }
    }
    Ok(frequency)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(lines: &str, characters: &str) -> KanjiFrequency {
        count(lines.as_bytes(), &characters.chars().collect()).unwrap()
    }

    #[test]
    fn ranks_by_count_and_ties_share_a_rank() {
        let lines = "1\tjpn\t日本の日。\tx\n2\tjpn\t本を読む日々。\t\\N\n3\tjpn\t山と川\tx\n";
        let frequency = counted(lines, "日本山川読");
        // 日 3, 本 2, then 読, 山, and 川 once each, in code point order. 々 is no kanji.
        assert_eq!(
            frequency.rows,
            [
                ('日' as u32, 1, 3),
                ('本' as u32, 2, 2),
                ('山' as u32, 3, 1),
                ('川' as u32, 3, 1),
                ('読' as u32, 3, 1),
            ]
        );
        assert_eq!(frequency.sentences, 3);
        assert_eq!(frequency.occurrences, 8);
    }

    #[test]
    fn a_kanji_outside_the_set_keeps_its_place_in_the_ranks() {
        let frequency = counted("1\tjpn\t犬犬猫\tx\n", "猫");
        assert_eq!(frequency.rows, [('猫' as u32, 2, 1)]);
        assert_eq!(frequency.outside_set, BTreeSet::from(['犬']));
    }

    #[test]
    fn counts_a_compatibility_ideograph_as_its_unified_kanji() {
        // U+F9DC is a compatibility form of 隆.
        let frequency = counted("1\tjpn\t\u{F9DC}隆\tx\n", "隆");
        assert_eq!(frequency.rows, [('隆' as u32, 1, 2)]);
    }

    #[test]
    fn reads_the_version_from_the_folder() {
        assert_eq!(
            version_from_path(Path::new("sources/tatoeba-2026-10-17")).as_deref(),
            Some("2026-10-17")
        );
        assert_eq!(version_from_path(Path::new("sources/other")), None);
    }
}
