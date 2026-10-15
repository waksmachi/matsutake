//! Stage 8: read the pitch accents of Kanjium. Give each reading its accents, each with the parts
//! of speech that it belongs to.

use std::collections::{HashMap, HashSet};
use std::io::BufRead;
use std::path::Path;

use anyhow::{Context, Result, bail};
use jpdag::kana::{morae, to_hiragana};

use crate::jmdict::Category;
use crate::source::Source;
use crate::words::{EntryRows, TagRow};

/// The file name of a Kanjium file that `fetch.sh` downloads, before the commit.
const FILE_PREFIX: &str = "kanjium-accents-";

/// The JMdict `pos` tag of each part of speech of Kanjium.
const PARTS_OF_SPEECH: [(&str, &str); 5] = [
    ("名", "n"),
    ("副", "adv"),
    ("形動", "adj-na"),
    ("代", "pn"),
    ("感", "int"),
];

/// 1 accent of a reading.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Accent {
    /// Number of morae before the fall in pitch, or 0 for no fall.
    downstep: u32,
    /// The JMdict `pos` tags of the parts of speech that the accent belongs to. Empty if the
    /// accent belongs to each.
    parts_of_speech: Vec<&'static str>,
}

/// The rows of stage 8, and what the build report says about the file.
#[derive(Debug, Default)]
pub struct Accents {
    /// Commit of Kanjium, from the file name, for example 685d4d7.
    pub version: Option<String>,
    /// Number of lines of the file.
    pub lines: usize,
    /// Number of lines that give no reading of `content.db` an accent.
    pub lines_without_word: usize,
    /// Number of accents that fall after the last mora of their reading.
    pub accents_past_the_end: usize,
    /// Number of readings that take the accents of another reading of their word.
    pub shared_readings: usize,
    /// `accent` rows, each as (id, reading_id, position, downstep).
    pub rows: Vec<(u32, u32, u32, u32)>,
    /// `accent_tag` rows, each as (accent_id, tag_id).
    pub tags: Vec<(u32, u32)>,
}

/// Gives the commit from a file name such as `kanjium-accents-685d4d7.txt`.
pub fn version_from_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let commit = stem.strip_prefix(FILE_PREFIX)?;
    (!commit.is_empty()).then(|| commit.to_owned())
}

/// Parses the accents of the line `number`, such as `(副)1,0,(形動)0`: 1 and 0 as an adverb, and
/// 0 as a na-adjective. A part of speech holds for each accent after it, up to the next part of
/// speech. An accent after the last mora of `reading` is counted in `past_the_end`, and skipped.
fn parse(field: &str, reading: &str, number: u64, past_the_end: &mut usize) -> Result<Vec<Accent>> {
    let length = morae(reading).len() as u32;
    let mut parts_of_speech: Vec<&'static str> = Vec::new();
    let mut accents: Vec<Accent> = Vec::new();
    for token in field.split(',') {
        let bad = || format!("Kanjium line {number}: bad accent {token:?}");
        let downstep = match token.strip_prefix('(') {
            Some(rest) => {
                let (names, downstep) = rest.split_once(')').with_context(bad)?;
                parts_of_speech = names
                    .split(';')
                    .map(|name| {
                        PARTS_OF_SPEECH
                            .iter()
                            .find(|&&(label, _)| label == name)
                            .map(|&(_, tag)| tag)
                            .with_context(|| {
                                format!("Kanjium line {number}: unknown part of speech {name:?}")
                            })
                    })
                    .collect::<Result<_>>()?;
                downstep
            }
            None => token,
        };
        let downstep: u32 = downstep.parse().with_context(bad)?;
        if downstep > length {
            *past_the_end += 1;
            continue;
        }
        // An accent with the downstep of an earlier accent joins it: (副)1,(感)1 is 1 accent of
        // an adverb and an interjection. An accent with no part of speech belongs to each, so the
        // joined accent has none.
        match accents.iter_mut().find(|a| a.downstep == downstep) {
            Some(accent) if accent.parts_of_speech.is_empty() => {}
            Some(accent) if parts_of_speech.is_empty() => accent.parts_of_speech.clear(),
            Some(accent) => {
                for &name in &parts_of_speech {
                    if !accent.parts_of_speech.contains(&name) {
                        accent.parts_of_speech.push(name);
                    }
                }
            }
            None => accents.push(Accent {
                downstep,
                parts_of_speech: parts_of_speech.clone(),
            }),
        }
    }
    Ok(accents)
}

/// Adds the accents of another written form to the accents of a reading. The first written form
/// gives all its accents. A later written form adds only the accents whose downstep the reading
/// does not have: the accents of 十月 are 4, and those of １０月 are 4 and 0, so じゅうがつ has 4
/// and 0.
fn merge(accents: &mut Vec<Accent>, more: &[Accent]) {
    if accents.is_empty() {
        accents.extend_from_slice(more);
        return;
    }
    for accent in more {
        if accents.iter().all(|a| a.downstep != accent.downstep) {
            accents.push(accent.clone());
        }
    }
}

/// Reads the accents of the readings, and gives the `accent` and `accent_tag` rows. Adds a `pos`
/// tag to `entries` if an accent needs a tag that no sense has, with its description from
/// `entities`.
///
/// Each line of the file is `<written form>\t<reading>\t<accents>`. A line of a word in kana alone
/// has the kana as the written form, and no reading.
pub fn read(
    mut input: Source,
    version: Option<String>,
    entries: &mut EntryRows,
    entities: &HashMap<String, String>,
) -> Result<Accents> {
    let mut result = Accents {
        version,
        ..Accents::default()
    };
    // The accents of each line, and its number of lines, by the written form and the reading
    // joined with a tab, or by the kana of a line with no reading.
    let mut of_pair: HashMap<String, (Vec<Accent>, usize)> = HashMap::new();
    let mut of_kana: HashMap<String, (Vec<Accent>, usize)> = HashMap::new();
    let mut line = String::new();
    loop {
        line.clear();
        let number = input.line();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        let text = line.trim_end_matches(['\n', '\r']);
        if text.is_empty() {
            continue;
        }
        result.lines += 1;
        let mut fields = text.splitn(3, '\t');
        let (Some(form), Some(reading), Some(field)) =
            (fields.next(), fields.next(), fields.next())
        else {
            bail!("Kanjium line {number}: not 3 fields: {text:?}");
        };
        let (map, key, kana) = if reading.is_empty() {
            (&mut of_kana, form.to_owned(), form)
        } else {
            (&mut of_pair, format!("{form}\t{reading}"), reading)
        };
        let accents = parse(field, kana, number, &mut result.accents_past_the_end)?;
        // A form and a reading that repeat keep the accents of their first line.
        map.entry(key).or_insert((accents, 0)).1 += 1;
    }

    let mut forms_of: HashMap<u32, Vec<(u32, &str)>> = HashMap::new();
    for form in &entries.written_forms {
        forms_of
            .entry(form.word_id)
            .or_default()
            .push((form.id, &form.text));
    }
    let mut restrictions: HashMap<u32, HashSet<u32>> = HashMap::new();
    for &(reading_id, form_id) in &entries.reading_restrictions {
        restrictions.entry(reading_id).or_default().insert(form_id);
    }
    let mut used: HashSet<String> = HashSet::new();
    let mut used_kana: HashSet<&str> = HashSet::new();
    // The accents of each reading, in the order of the readings.
    let mut accents_of: Vec<(u32, u32, &str, Vec<Accent>)> = Vec::new();
    for reading in &entries.readings {
        let forms: Vec<&str> = if reading.no_kanji {
            Vec::new()
        } else {
            let allowed = restrictions.get(&reading.id);
            forms_of
                .get(&reading.word_id)
                .into_iter()
                .flatten()
                .filter(|(id, _)| allowed.is_none_or(|allowed| allowed.contains(id)))
                .map(|&(_, text)| text)
                .collect()
        };
        let mut accents = Vec::new();
        if forms.is_empty() {
            // The line of a word in kana alone gives the accents of a reading of no written form.
            if let Some((list, _)) = of_kana.get(&reading.text) {
                used_kana.insert(&reading.text);
                accents.clone_from(list);
            }
        }
        for form in forms {
            let key = format!("{form}\t{}", reading.text);
            if let Some((list, _)) = of_pair.get(&key) {
                merge(&mut accents, list);
                used.insert(key);
            }
        }
        accents_of.push((reading.id, reading.word_id, &reading.text, accents));
    }
    result.lines_without_word = of_pair
        .iter()
        .filter(|(key, _)| !used.contains(*key))
        .chain(
            of_kana
                .iter()
                .filter(|(kana, _)| !used_kana.contains(kana.as_str())),
        )
        .map(|(_, &(_, lines))| lines)
        .sum();

    // A reading with no accents takes the accents of the first reading of its word that is the
    // same in hiragana: チョット takes the accents of ちょっと.
    let mut first_of: HashMap<(u32, String), usize> = HashMap::new();
    for (index, (_, word_id, text, accents)) in accents_of.iter().enumerate() {
        if !accents.is_empty() {
            first_of
                .entry((*word_id, to_hiragana(text)))
                .or_insert(index);
        }
    }
    let mut shared = Vec::new();
    for (index, (_, word_id, text, accents)) in accents_of.iter().enumerate() {
        if accents.is_empty()
            && let Some(&other) = first_of.get(&(*word_id, to_hiragana(text)))
        {
            shared.push((index, other));
        }
    }
    result.shared_readings = shared.len();
    let mut accents_of: Vec<(u32, Vec<Accent>)> = accents_of
        .into_iter()
        .map(|(reading_id, _, _, accents)| (reading_id, accents))
        .collect();
    for (index, other) in shared {
        accents_of[index].1 = accents_of[other].1.clone();
    }

    let mut tag_ids: HashMap<&str, u32> = HashMap::new();
    for (reading_id, accents) in accents_of {
        for (position, accent) in accents.iter().enumerate() {
            let id = result.rows.len() as u32 + 1;
            result
                .rows
                .push((id, reading_id, position as u32 + 1, accent.downstep));
            for &name in &accent.parts_of_speech {
                let tag_id = *tag_ids
                    .entry(name)
                    .or_insert_with(|| pos_tag_id(entries, entities, name));
                result.tags.push((id, tag_id));
            }
        }
    }
    Ok(result)
}

/// Gives the id of the `pos` tag `name`, and adds the tag if `entries` has none.
fn pos_tag_id(entries: &mut EntryRows, entities: &HashMap<String, String>, name: &str) -> u32 {
    if let Some(tag) = entries
        .tags
        .iter()
        .find(|tag| tag.category == Category::Pos && tag.name == name)
    {
        return tag.id;
    }
    let id = entries.tags.len() as u32 + 1;
    entries.tags.push(TagRow {
        id,
        category: Category::Pos,
        name: name.to_owned(),
        description: entities.get(name).cloned(),
    });
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source;
    use crate::words::{ReadingRow, WrittenFormRow};

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

    fn run(file: &str, entries: &mut EntryRows) -> Result<Accents> {
        let input = source::open(std::io::Cursor::new(file.as_bytes().to_vec())).unwrap();
        let entities = HashMap::from([("adv".to_owned(), "adverb (fukushi)".to_owned())]);
        read(input, None, entries, &entities)
    }

    #[test]
    fn each_reading_gets_the_accents_of_its_form() {
        let mut words = entries(&[(1, &["橋"], &["はし"]), (2, &["箸"], &["はし"])]);
        let accents = run("橋\tはし\t2\n箸\tはし\t1\n", &mut words).unwrap();
        assert_eq!(accents.lines, 2);
        assert_eq!(accents.rows, [(1, 1, 1, 2), (2, 2, 1, 1)]);
        assert!(accents.tags.is_empty());
    }

    #[test]
    fn variants_keep_their_order() {
        let mut words = entries(&[(1, &["１０月"], &["じゅうがつ"])]);
        let accents = run("１０月\tじゅうがつ\t4,0\n", &mut words).unwrap();
        assert_eq!(accents.rows, [(1, 1, 1, 4), (2, 1, 2, 0)]);
    }

    #[test]
    fn later_forms_add_only_new_downsteps() {
        let mut words = entries(&[(1, &["十月", "１０月"], &["じゅうがつ"])]);
        let accents = run("１０月\tじゅうがつ\t4,0\n十月\tじゅうがつ\t4\n", &mut words).unwrap();
        // 十月 is the first form, so its 4 comes first.
        assert_eq!(accents.rows, [(1, 1, 1, 4), (2, 1, 2, 0)]);
    }

    #[test]
    fn parts_of_speech_become_pos_tags() {
        let mut words = entries(&[(1, &["二つ"], &["ふたつ"])]);
        words.tags.push(TagRow {
            id: 1,
            category: Category::Pos,
            name: "n".into(),
            description: Some("noun (common) (futsuumeishi)".into()),
        });
        let accents = run("二つ\tふたつ\t(副)1,0,(名)3\n", &mut words).unwrap();
        assert_eq!(accents.rows, [(1, 1, 1, 1), (2, 1, 2, 0), (3, 1, 3, 3)]);
        // adv is a new tag, with its description from the entities. n is the tag of the senses.
        assert_eq!(accents.tags, [(1, 2), (2, 2), (3, 1)]);
        assert_eq!(words.tags[1].name, "adv");
        assert_eq!(
            words.tags[1].description.as_deref(),
            Some("adverb (fukushi)")
        );
    }

    #[test]
    fn accents_of_the_same_downstep_join() {
        let mut words = entries(&[(1, &["一寸"], &["ちょいと"]), (2, &["二つ"], &["ふたつ"])]);
        let accents = run(
            "一寸\tちょいと\t(副)0,1,(感)1\n二つ\tふたつ\t3,(名)3\n",
            &mut words,
        )
        .unwrap();
        // ちょいと is 0 as an adverb, and 1 as an adverb and an interjection. ふたつ is 3 for
        // each part of speech.
        assert_eq!(accents.rows, [(1, 1, 1, 0), (2, 1, 2, 1), (3, 2, 1, 3)]);
        assert_eq!(words.tags[0].name, "adv");
        assert_eq!(words.tags[1].name, "int");
        assert_eq!(accents.tags, [(1, 1), (2, 1), (2, 2)]);
    }

    #[test]
    fn several_parts_of_speech_share_an_accent() {
        let mut words = entries(&[(1, &["一寸"], &["ちょっと"])]);
        let accents = run("一寸\tちょっと\t(名;副)1\n", &mut words).unwrap();
        assert_eq!(accents.tags, [(1, 1), (1, 2)]);
        assert_eq!(words.tags[0].name, "n");
        assert_eq!(words.tags[1].name, "adv");
    }

    #[test]
    fn unknown_part_of_speech_fails() {
        let mut words = entries(&[(1, &["二つ"], &["ふたつ"])]);
        let error = run("二つ\tふたつ\t1\n二つ\tふたつ\t(動)1\n", &mut words)
            .unwrap_err()
            .to_string();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn kana_line_gives_the_accents_of_a_reading_of_no_form() {
        let mut words = entries(&[(1, &[], &["ああ"]), (2, &["嗚呼"], &["ああ"])]);
        let accents = run("ああ\t\t0\n", &mut words).unwrap();
        // The reading of 嗚呼 reads a written form, so the line is not its accents.
        assert_eq!(accents.rows, [(1, 1, 1, 0)]);
    }

    #[test]
    fn reading_reads_only_its_restricted_forms() {
        let mut words = entries(&[(1, &["一寸", "鳥渡"], &["ちょと"])]);
        words.reading_restrictions.push((1, 2));
        let accents = run("一寸\tちょと\t1\n", &mut words).unwrap();
        assert!(accents.rows.is_empty());
        assert_eq!(accents.lines_without_word, 1);
    }

    #[test]
    fn katakana_reading_shares_the_accents_of_its_hiragana() {
        let mut words = entries(&[(1, &["鳥"], &["とり", "トリ"])]);
        words.readings[1].no_kanji = true;
        let accents = run("鳥\tとり\t0\n", &mut words).unwrap();
        assert_eq!(accents.rows, [(1, 1, 1, 0), (2, 2, 1, 0)]);
        assert_eq!(accents.shared_readings, 1);
    }

    #[test]
    fn accent_past_the_end_is_counted_and_skipped() {
        let mut words = entries(&[(1, &["言論機関"], &["げんろんきかん"])]);
        let accents = run("言論機関\tげんろんきかん\t8,7\n", &mut words).unwrap();
        assert_eq!(accents.rows, [(1, 1, 1, 7)]);
        assert_eq!(accents.accents_past_the_end, 1);
    }

    #[test]
    fn line_of_no_word_is_counted() {
        let mut words = entries(&[(1, &["橋"], &["はし"])]);
        let accents = run("端\tはし\t0\nはし\t\t0\n", &mut words).unwrap();
        assert!(accents.rows.is_empty());
        assert_eq!(accents.lines_without_word, 2);
    }

    #[test]
    fn line_of_2_fields_fails() {
        let mut words = entries(&[]);
        let error = run("橋\tはし\n", &mut words).unwrap_err().to_string();
        assert!(error.contains("line 1"), "{error}");
    }

    #[test]
    fn version_comes_from_the_file_name() {
        assert_eq!(
            version_from_path(Path::new("sources/kanjium-accents-685d4d7.txt")).as_deref(),
            Some("685d4d7")
        );
        assert_eq!(version_from_path(Path::new("accents.txt")), None);
    }
}
