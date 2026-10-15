//! Stage 7: read the furigana of JmdictFurigana. Keep the furigana of each written form and
//! reading, and count the words that read each character with each of its readings.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::BufRead;
use std::path::Path;

use anyhow::{Context, Result, bail};
use jpdag::kana::to_hiragana;
use jpdag::normalize::normalize_char;

use crate::kanjidic::{Kanjidic, ReadingType};
use crate::source::Source;
use crate::words::EntryRows;

/// The file name of a JmdictFurigana file that `fetch.sh` downloads, before the date of the
/// release.
const FILE_PREFIX: &str = "JmdictFurigana-";

/// The counts of stage 7, and what the build report says about the file.
#[derive(Debug, Default)]
pub struct Furigana {
    /// Date of the release, from the file name, for example 2026-09-25.
    pub version: Option<String>,
    /// Number of lines of the file.
    pub lines: usize,
    /// Number of lines whose written form and reading are not a word of `content.db`.
    pub lines_without_word: usize,
    /// Number of parts of the furigana that give 1 kanji a reading that is not 1 of its
    /// KANJIDIC2 readings, for example an irregular reading.
    pub parts_without_reading: usize,
    /// The words that read each character with the reading at each position.
    words: BTreeMap<(char, u32), BTreeSet<u32>>,
    /// `furigana` rows, each as (written_form_id, reading_id, position, start, length, kana).
    pub rows: Vec<(u32, u32, u32, u32, u32, String)>,
}

impl Furigana {
    /// Number of words that read `character` with its reading at `position`.
    pub fn word_count(&self, character: char, position: u32) -> u32 {
        self.words
            .get(&(character, position))
            .map_or(0, |words| words.len() as u32)
    }

    /// Number of readings that 1 or more words use.
    pub fn readings_in_use(&self) -> usize {
        self.words.len()
    }
}

/// A written form and a reading of a word, as (word_id, written_form_id, reading_id).
type Pair = (u32, u32, u32);

/// Gives the date of the release from a file name such as `JmdictFurigana-2026-09-25.txt`.
pub fn version_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let date = stem.strip_prefix(FILE_PREFIX)?;
    (!date.is_empty()).then(|| date.to_owned())
}

/// A KANJIDIC2 reading, prepared for the comparison with the furigana.
struct Candidate {
    /// Position of the reading in its KANJIDIC2 entry. The first position is 1.
    position: u32,
    /// The reading in hiragana, without its okurigana and without `-`.
    stem: String,
    /// The okurigana of a kun reading: the text after the `.`.
    okurigana: String,
}

impl Candidate {
    fn new(position: u32, kind: ReadingType, text: &str) -> Candidate {
        let text: String = text.chars().filter(|&c| c != '-').collect();
        let (stem, okurigana) = match kind {
            ReadingType::On => (to_hiragana(&text), String::new()),
            ReadingType::Kun => match text.split_once('.') {
                Some((stem, okurigana)) => (stem.to_owned(), okurigana.to_owned()),
                None => (text, String::new()),
            },
        };
        Candidate {
            position,
            stem,
            okurigana,
        }
    }

    /// Whether the kana of a part of the furigana are this reading. The kana can have the sound
    /// changes of a compound: a voiced first kana (rendaku, ぼん for ほん in 三本) and a small っ
    /// for the last kana (sokuon, がっ for がく in 学校).
    fn matches(&self, kana: &str) -> bool {
        let stems = [Some(self.stem.clone()), sokuon(&self.stem)];
        stems
            .into_iter()
            .flatten()
            .any(|stem| stem == kana || rendaku(&stem).contains(&kana.to_owned()))
    }
}

/// Gives `stem` with a small っ for its last kana, if the last kana can become っ.
fn sokuon(stem: &str) -> Option<String> {
    let last = stem.chars().last()?;
    matches!(last, 'つ' | 'ち' | 'く' | 'き')
        .then(|| format!("{}っ", &stem[..stem.len() - last.len_utf8()]))
}

/// Gives `stem` with each voiced form of its first kana.
fn rendaku(stem: &str) -> Vec<String> {
    let mut chars = stem.chars();
    let Some(first) = chars.next() else {
        return Vec::new();
    };
    let rest = chars.as_str();
    voiced(first)
        .iter()
        .map(|voiced| format!("{voiced}{rest}"))
        .collect()
}

/// The voiced forms of a kana: with a dakuten, and for the h row also with a handakuten.
fn voiced(kana: char) -> &'static [char] {
    match kana {
        'か' => &['が'],
        'き' => &['ぎ'],
        'く' => &['ぐ'],
        'け' => &['げ'],
        'こ' => &['ご'],
        'さ' => &['ざ'],
        'し' => &['じ'],
        'す' => &['ず'],
        'せ' => &['ぜ'],
        'そ' => &['ぞ'],
        'た' => &['だ'],
        'ち' => &['ぢ', 'じ'],
        'つ' => &['づ', 'ず'],
        'て' => &['で'],
        'と' => &['ど'],
        'は' => &['ば', 'ぱ'],
        'ひ' => &['び', 'ぴ'],
        'ふ' => &['ぶ', 'ぷ'],
        'へ' => &['べ', 'ぺ'],
        'ほ' => &['ぼ', 'ぽ'],
        _ => &[],
    }
}

/// Gives the number of characters that `a` and `b` share at their start.
fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Reads the furigana of the words. Keeps the furigana of each written form and reading of a
/// word, and counts the words that use each reading of each character of `kanjidic`.
///
/// Each line of the file is `<written form>|<reading>|<furigana>`. The furigana are parts such
/// as `0:ご;1:がく`: the index of the character in the written form, then its kana. A part for a
/// range of characters, such as `0-1:きょう` for 今日, reads no single kanji: it is a furigana row,
/// and the counts skip it.
pub fn count(
    mut input: Source,
    version: Option<String>,
    entries: &EntryRows,
    kanjidic: &Kanjidic,
) -> Result<Furigana> {
    // Each written form and reading of a word, by the texts of the form and the reading.
    let mut pairs_of: HashMap<(&str, &str), Vec<Pair>> = HashMap::new();
    let mut readings_of: HashMap<u32, Vec<(u32, &str)>> = HashMap::new();
    for reading in &entries.readings {
        readings_of
            .entry(reading.word_id)
            .or_default()
            .push((reading.id, &reading.text));
    }
    for form in &entries.written_forms {
        for &(reading_id, reading) in readings_of.get(&form.word_id).into_iter().flatten() {
            pairs_of.entry((&form.text, reading)).or_default().push((
                form.word_id,
                form.id,
                reading_id,
            ));
        }
    }
    let mut pairs_with_rows = HashSet::new();
    let candidates: HashMap<char, Vec<Candidate>> = kanjidic
        .entries
        .iter()
        .map(|(&c, entry)| {
            let readings = entry.readings.iter().enumerate();
            let list = readings
                .map(|(i, (kind, text))| Candidate::new(i as u32 + 1, *kind, text))
                .collect();
            (c, list)
        })
        .collect();

    let mut furigana = Furigana {
        version,
        ..Furigana::default()
    };
    let mut line = String::new();
    loop {
        line.clear();
        let number = input.line();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        let text = line.trim_start_matches('\u{FEFF}').trim_end();
        if text.is_empty() {
            continue;
        }
        furigana.lines += 1;
        let mut fields = text.splitn(3, '|');
        let (Some(form), Some(reading), Some(parts)) =
            (fields.next(), fields.next(), fields.next())
        else {
            bail!("JmdictFurigana line {number}: not 3 fields: {text:?}");
        };
        let Some(pairs) = pairs_of.get(&(form, reading)) else {
            furigana.lines_without_word += 1;
            continue;
        };
        let characters: Vec<char> = form.chars().collect();
        // Each part as (start, length, kana).
        let mut parsed = Vec::new();
        for part in parts.split(';').filter(|part| !part.is_empty()) {
            let bad = || format!("JmdictFurigana line {number}: bad part {part:?}");
            let (range, kana) = part.split_once(':').with_context(bad)?;
            let (first, last) = range.split_once('-').unwrap_or((range, range));
            let (first, last): (usize, usize) = (
                first.parse().with_context(bad)?,
                last.parse().with_context(bad)?,
            );
            if first > last || last >= characters.len() {
                bail!("JmdictFurigana line {number}: no characters {range} in {form:?}");
            }
            parsed.push((first, last - first + 1, kana));
        }
        for &(_, form_id, reading_id) in pairs {
            // A written form and a reading get the furigana of their first line.
            if pairs_with_rows.insert((form_id, reading_id)) {
                for (i, &(start, length, kana)) in parsed.iter().enumerate() {
                    furigana.rows.push((
                        form_id,
                        reading_id,
                        i as u32 + 1,
                        start as u32,
                        length as u32,
                        kana.to_owned(),
                    ));
                }
            }
        }
        // A part of a range of characters, such as 今日, reads no single kanji.
        for &(index, _, kana) in parsed.iter().filter(|&&(_, length, _)| length == 1) {
            let Some(character) = normalize_char(characters[index]) else {
                continue;
            };
            let Some(list) = candidates.get(&character) else {
                continue;
            };
            let after: String = characters[index + 1..].iter().collect();
            // Kun readings can share a stem, for example い.きる and い.かす of 生. The okurigana
            // in the written form decide between them. A tie goes to the first reading.
            let best = list
                .iter()
                .filter(|candidate| candidate.matches(kana))
                .max_by_key(|candidate| {
                    (
                        common_prefix(&candidate.okurigana, &after),
                        std::cmp::Reverse(candidate.position),
                    )
                });
            match best {
                Some(candidate) => {
                    let words = furigana
                        .words
                        .entry((character, candidate.position))
                        .or_default();
                    words.extend(pairs.iter().map(|&(word_id, _, _)| word_id));
                }
                None => furigana.parts_without_reading += 1,
            }
        }
    }
    Ok(furigana)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kanjidic::Entry;
    use crate::source;
    use crate::words::{ReadingRow, WrittenFormRow};

    fn entries(words: &[(u32, &str, &str)]) -> EntryRows {
        let mut rows = EntryRows::default();
        for (i, &(word_id, form, reading)) in words.iter().enumerate() {
            let id = i as u32 + 1;
            rows.words.push(word_id);
            rows.written_forms.push(WrittenFormRow {
                id,
                word_id,
                position: 1,
                text: form.into(),
            });
            rows.readings.push(ReadingRow {
                id,
                word_id,
                position: 1,
                text: reading.into(),
                no_kanji: false,
            });
        }
        rows
    }

    fn kanjidic(entries: &[(char, &[(ReadingType, &str)])]) -> Kanjidic {
        let mut kanjidic = Kanjidic::default();
        for &(c, readings) in entries {
            kanjidic.entries.insert(
                c,
                Entry {
                    readings: readings
                        .iter()
                        .map(|&(kind, text)| (kind, text.to_owned()))
                        .collect(),
                    meanings: Vec::new(),
                },
            );
        }
        kanjidic
    }

    fn run(file: &str, words: &[(u32, &str, &str)], kanjidic: &Kanjidic) -> Furigana {
        let input = source::open(std::io::Cursor::new(file.as_bytes().to_vec())).unwrap();
        count(input, None, &entries(words), kanjidic).unwrap()
    }

    use ReadingType::{Kun, On};

    #[test]
    fn counts_the_words_of_each_reading() {
        let kanjidic = kanjidic(&[('語', &[(On, "ゴ"), (Kun, "かた.る")])]);
        let furigana = run(
            "\u{FEFF}語学|ごがく|0:ご;1:がく\n語る|かたる|0:かた\n英語|えいご|0:えい;1:ご\n",
            &[
                (1, "語学", "ごがく"),
                (2, "語る", "かたる"),
                (3, "英語", "えいご"),
            ],
            &kanjidic,
        );
        assert_eq!(furigana.lines, 3);
        assert_eq!(furigana.word_count('語', 1), 2);
        assert_eq!(furigana.word_count('語', 2), 1);
    }

    #[test]
    fn line_of_no_word_is_counted_and_skipped() {
        let kanjidic = kanjidic(&[('語', &[(On, "ゴ")])]);
        let furigana = run("語学|ごがく|0:ご;1:がく\n", &[], &kanjidic);
        assert_eq!(furigana.lines_without_word, 1);
        assert_eq!(furigana.word_count('語', 1), 0);
    }

    #[test]
    fn voiced_first_kana_and_small_tsu_match() {
        let kanjidic = kanjidic(&[('学', &[(On, "ガク")]), ('本', &[(On, "ホン")])]);
        let furigana = run(
            "学校|がっこう|0:がっ;1:こう\n三本|さんぼん|0:さん;1:ぼん\n",
            &[(1, "学校", "がっこう"), (2, "三本", "さんぼん")],
            &kanjidic,
        );
        assert_eq!(furigana.word_count('学', 1), 1);
        assert_eq!(furigana.word_count('本', 1), 1);
    }

    #[test]
    fn okurigana_decide_between_kun_readings() {
        let kanjidic = kanjidic(&[('生', &[(Kun, "い.きる"), (Kun, "い.かす")])]);
        let furigana = run(
            "生かす|いかす|0:い\n生きる|いきる|0:い\n",
            &[(1, "生かす", "いかす"), (2, "生きる", "いきる")],
            &kanjidic,
        );
        assert_eq!(furigana.word_count('生', 1), 1);
        assert_eq!(furigana.word_count('生', 2), 1);
    }

    #[test]
    fn irregular_reading_is_reported() {
        let kanjidic = kanjidic(&[('今', &[(On, "コン")]), ('日', &[(On, "ニチ")])]);
        let furigana = run(
            "今日|きょう|0-1:きょう\n今日|こんにち|0:こん;1:にち\n日向|ひなた|0:ひ;1:なた\n",
            &[
                (1, "今日", "きょう"),
                (2, "今日", "こんにち"),
                (3, "日向", "ひなた"),
            ],
            &kanjidic,
        );
        assert_eq!(furigana.word_count('今', 1), 1);
        assert_eq!(furigana.word_count('日', 1), 1);
        // ひ is not a reading of 日 in this KANJIDIC2, and 向 has no entry.
        assert_eq!(furigana.parts_without_reading, 1);
    }

    #[test]
    fn a_word_counts_once_for_each_reading() {
        let kanjidic = kanjidic(&[('寸', &[(On, "スン")])]);
        let mut words = entries(&[(1, "一寸", "ちょっと")]);
        words.readings.push(ReadingRow {
            id: 2,
            word_id: 1,
            position: 2,
            text: "いっすん".into(),
            no_kanji: false,
        });
        let input = source::open(std::io::Cursor::new(
            "一寸|いっすん|0:いっ;1:すん\n一寸|いっすん|0:いっ;1:すん\n"
                .as_bytes()
                .to_vec(),
        ))
        .unwrap();
        let furigana = count(input, None, &words, &kanjidic).unwrap();
        assert_eq!(furigana.word_count('寸', 1), 1);
    }

    #[test]
    fn parts_become_furigana_rows() {
        let kanjidic = kanjidic(&[('語', &[(On, "ゴ")])]);
        let furigana = run(
            "語る|かたる|0:かた\n今日|きょう|0-1:きょう\n今日|きょう|0-1:きょう\n",
            &[(1, "語る", "かたる"), (2, "今日", "きょう")],
            &kanjidic,
        );
        // The second line of 今日 gives no rows: the form and the reading have furigana.
        assert_eq!(
            furigana.rows,
            [
                (1, 1, 1, 0, 1, "かた".to_owned()),
                (2, 2, 1, 0, 2, "きょう".to_owned()),
            ]
        );
    }

    #[test]
    fn part_outside_the_form_fails() {
        let input = source::open(std::io::Cursor::new("語|ご|1:ご\n".as_bytes().to_vec())).unwrap();
        let error = count(
            input,
            None,
            &entries(&[(1, "語", "ご")]),
            &Kanjidic::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("line 1"), "{error}");
    }

    #[test]
    fn version_comes_from_the_file_name() {
        assert_eq!(
            version_from_path(Path::new("sources/JmdictFurigana-2026-09-25.txt")).as_deref(),
            Some("2026-09-25")
        );
        assert_eq!(version_from_path(Path::new("JmdictFurigana.txt")), None);
    }
}
