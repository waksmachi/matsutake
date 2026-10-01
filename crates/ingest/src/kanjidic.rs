//! Stage 6: read KANJIDIC2.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, anyhow, bail};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::source::Source;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingType {
    On,
    Kun,
}

impl ReadingType {
    /// Value of `character_reading.type`.
    pub fn as_str(self) -> &'static str {
        match self {
            ReadingType::On => "on",
            ReadingType::Kun => "kun",
        }
    }

    /// Type of a `<reading>` with the `r_type` value, or `None` if stage 6 discards the reading.
    fn of_r_type(r_type: &str) -> Option<Self> {
        match r_type {
            "ja_on" => Some(ReadingType::On),
            "ja_kun" => Some(ReadingType::Kun),
            _ => None,
        }
    }
}

/// A KANJIDIC2 entry.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Entry {
    /// On readings and kun readings, in file order.
    pub readings: Vec<(ReadingType, String)>,
    /// English meanings, in file order.
    pub meanings: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Kanjidic {
    /// KANJIDIC2 entry of each character of the character set that has one.
    pub entries: BTreeMap<char, Entry>,
    /// Number of `<character>` elements that stage 6 ignores.
    pub ignored: usize,
    /// Text of `<date_of_creation>` in the `<header>` element.
    pub version: Option<String>,
}

impl Kanjidic {
    /// `character_meaning` rows, each as (character_id, position, text).
    pub fn meaning_rows(&self) -> Vec<(u32, u32, String)> {
        let rows = |(&c, entry): (&char, &Entry)| {
            let meanings = entry.meanings.clone().into_iter().enumerate();
            meanings.map(move |(i, text)| (c as u32, i as u32 + 1, text))
        };
        self.entries.iter().flat_map(rows).collect()
    }

    /// `character_reading` rows, each as (character_id, position, type, text).
    pub fn reading_rows(&self) -> Vec<(u32, u32, &'static str, String)> {
        let rows = |(&c, entry): (&char, &Entry)| {
            let readings = entry.readings.clone().into_iter().enumerate();
            readings.map(move |(i, (kind, text))| (c as u32, i as u32 + 1, kind.as_str(), text))
        };
        self.entries.iter().flat_map(rows).collect()
    }
}

fn single_char(s: &str) -> Option<char> {
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

fn attribute(element: &BytesStart, key: &str) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .map(|a| {
            a.normalized_value(XmlVersion::Implicit1_0)
                .unwrap_or_else(|_| a.value.clone())
                .into_owned()
        })
}

/// Parses the KANJIDIC2 data of `input`, and keeps the entries of `characters`, the character set
/// of stage 5.
pub fn parse(input: Source, characters: &BTreeSet<char>) -> Result<Kanjidic> {
    let mut reader = Reader::from_reader(input);
    let mut buf = Vec::new();
    let mut result = Kanjidic::default();

    // The open elements, each with the line of its start tag.
    let mut open: Vec<(String, u64)> = Vec::new();
    // The open `<character>` element, and its literal if the character set holds it.
    let mut entry: Option<Entry> = None;
    let mut literal: Option<char> = None;
    // The text inside the innermost element.
    let mut text = String::new();
    // The type of the open `<reading>` element, and whether the open `<meaning>` is English.
    let mut reading_type: Option<ReadingType> = None;
    let mut english = false;

    loop {
        let line = reader.get_ref().line();
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| anyhow!("KANJIDIC2 line {}: {e}", reader.get_ref().line()))?;
        match event {
            Event::Start(e) => {
                let name = e.name().as_ref().to_owned();
                match name.as_str() {
                    "character" => {
                        entry = Some(Entry::default());
                        literal = None;
                    }
                    "reading" => {
                        reading_type =
                            attribute(&e, "r_type").and_then(|t| ReadingType::of_r_type(&t));
                    }
                    "meaning" => english = attribute(&e, "m_lang").is_none(),
                    _ => {}
                }
                open.push((name, line));
                text.clear();
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::CData(t) => text.push_str(&t.into_inner()),
            Event::GeneralRef(r) => {
                if r.is_char_ref() {
                    if let Some(c) = r
                        .resolve_char_ref()
                        .map_err(|e| anyhow!("KANJIDIC2 line {line}: {e}"))?
                    {
                        text.push(c);
                    }
                } else if let Some(value) = resolve_predefined_entity(&r) {
                    text.push_str(value);
                }
            }
            Event::End(_) => {
                let (name, start_line) = open.pop().expect("quick-xml checks the end names");
                let value = text.trim();
                match (name.as_str(), entry.as_mut()) {
                    ("date_of_creation", None) => {
                        if result.version.is_none() && !value.is_empty() {
                            result.version = Some(value.to_owned());
                        }
                    }
                    // Step 3. The literal is not normalized.
                    ("literal", Some(_)) => {
                        literal = single_char(value).filter(|c| characters.contains(c));
                    }
                    ("reading", Some(e)) => {
                        if let Some(kind) = reading_type.take() {
                            e.readings.push((kind, value.to_owned()));
                        }
                    }
                    ("meaning", Some(e)) => {
                        if english {
                            e.meanings.push(value.to_owned());
                        }
                    }
                    ("character", Some(_)) => {
                        let done = entry.take().expect("inside a character");
                        match literal.take() {
                            Some(c) => {
                                if result.entries.insert(c, done).is_some() {
                                    bail!(
                                        "KANJIDIC2 line {start_line}: second entry of the character {c}"
                                    );
                                }
                            }
                            None => result.ignored += 1,
                        }
                    }
                    _ => {}
                }
                text.clear();
            }
            Event::Eof => {
                if let Some((name, start_line)) = open.last() {
                    bail!("KANJIDIC2 line {start_line}: element <{name}> is not closed");
                }
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source;
    use std::io::{Cursor, Write};

    const HEADER: &str = "<header>\n<file_version>4</file_version>\n\
                          <date_of_creation>2026-10-01</date_of_creation>\n</header>\n";

    /// Entry of 語 from section 6.7 of the design.
    const GO: &str = r#"<character>
<literal>語</literal>
<misc>
<grade>2</grade>
<stroke_count>14</stroke_count>
</misc>
<reading_meaning>
<rmgroup>
<reading r_type="pinyin">yu3</reading>
<reading r_type="ja_on">ゴ</reading>
<reading r_type="ja_kun">かた.る</reading>
<reading r_type="ja_kun">かた.らう</reading>
<meaning>word</meaning>
<meaning>speech</meaning>
<meaning>language</meaning>
<meaning m_lang="fr">mot</meaning>
</rmgroup>
</reading_meaning>
</character>"#;

    fn doc(body: &str) -> String {
        format!("<kanjidic2>\n{HEADER}{body}\n</kanjidic2>\n")
    }

    fn parse_in(body: &str, characters: &[char]) -> Result<Kanjidic> {
        let characters = characters.iter().copied().collect();
        parse(
            source::open(Cursor::new(doc(body).into_bytes())).unwrap(),
            &characters,
        )
    }

    fn entry(body: &str, c: char) -> Entry {
        let kanjidic = parse_in(body, &[c]).unwrap();
        kanjidic
            .entries
            .get(&c)
            .cloned()
            .unwrap_or_else(|| panic!("no entry for {c}"))
    }

    /// An entry with the literal `c` and the elements `inner` in its `<rmgroup>`.
    fn character(c: char, inner: &str) -> String {
        format!(
            "<character>\n<literal>{c}</literal>\n<reading_meaning>\n<rmgroup>\n{inner}\n</rmgroup>\n\
             </reading_meaning>\n</character>"
        )
    }

    fn go_expected() -> Entry {
        Entry {
            readings: vec![
                (ReadingType::On, "ゴ".into()),
                (ReadingType::Kun, "かた.る".into()),
                (ReadingType::Kun, "かた.らう".into()),
            ],
            meanings: vec!["word".into(), "speech".into(), "language".into()],
        }
    }

    #[test]
    fn keeps_the_details_of_an_entry() {
        assert_eq!(entry(GO, '語'), go_expected());
    }

    #[test]
    fn readings_keep_their_order_and_type() {
        let kanjidic = parse_in(GO, &['語']).unwrap();
        assert_eq!(
            kanjidic.reading_rows(),
            vec![
                (0x8A9E, 1, "on", "ゴ".to_owned()),
                (0x8A9E, 2, "kun", "かた.る".to_owned()),
                (0x8A9E, 3, "kun", "かた.らう".to_owned()),
            ]
        );
    }

    #[test]
    fn other_reading_types_are_discarded() {
        let body = character(
            '語',
            r#"<reading r_type="pinyin">yu3</reading>
<reading r_type="korean_r">eo</reading>
<reading r_type="korean_h">어</reading>
<reading r_type="vietnam">Ngữ</reading>"#,
        );
        assert!(entry(&body, '語').readings.is_empty());
    }

    #[test]
    fn meanings_of_other_languages_are_discarded() {
        let body = character(
            '語',
            r#"<meaning>word</meaning>
<meaning m_lang="fr">mot</meaning>
<meaning>speech</meaning>"#,
        );
        let kanjidic = parse_in(&body, &['語']).unwrap();
        assert_eq!(
            kanjidic.meaning_rows(),
            vec![
                (0x8A9E, 1, "word".to_owned()),
                (0x8A9E, 2, "speech".to_owned()),
            ]
        );
    }

    #[test]
    fn entry_without_readings_and_meanings() {
        let body = "<character>\n<literal>語</literal>\n</character>";
        assert_eq!(entry(body, '語'), Entry::default());
    }

    #[test]
    fn nanori_is_discarded() {
        let body = "<character>\n<literal>生</literal>\n<reading_meaning>\n<rmgroup>\n\
                    <reading r_type=\"ja_on\">セイ</reading>\n</rmgroup>\n<nanori>あさ</nanori>\n\
                    </reading_meaning>\n</character>";
        assert_eq!(
            entry(body, '生').readings,
            vec![(ReadingType::On, "セイ".to_owned())]
        );
    }

    #[test]
    fn character_outside_the_set_is_ignored() {
        let kanjidic = parse_in(GO, &['休']).unwrap();
        assert!(kanjidic.entries.is_empty());
        assert_eq!(kanjidic.ignored, 1);
    }

    #[test]
    fn literal_is_not_normalized() {
        // U+FA19 normalizes to 神 (U+795E).
        let body = character('\u{FA19}', "<meaning>spirit</meaning>");
        let kanjidic = parse_in(&body, &['神']).unwrap();
        assert!(kanjidic.entries.is_empty());
        assert_eq!(kanjidic.ignored, 1);
    }

    #[test]
    fn entry_of_a_supplementary_plane() {
        let body = character('\u{20B9F}', "<meaning>scold</meaning>");
        assert_eq!(entry(&body, '\u{20B9F}').meanings, vec!["scold".to_owned()]);
    }

    #[test]
    fn meaning_text_is_unescaped() {
        let body = character('左', "<meaning>left &amp; right</meaning>");
        assert_eq!(entry(&body, '左').meanings, vec!["left & right".to_owned()]);
    }

    #[test]
    fn duplicate_entry_fails() {
        // The second <character> element is on line 3 of the body.
        let body = format!(
            "{}\n{}",
            "<character><literal>語</literal></character>\n",
            "<character><literal>語</literal></character>"
        );
        let error = parse_in(&body, &['語']).unwrap_err().to_string();
        let line = 1 + HEADER.lines().count() + 3;
        assert!(
            error.contains(&format!("line {line}")) && error.contains('語'),
            "{error}"
        );
    }

    #[test]
    fn creation_date_gives_the_version() {
        let kanjidic = parse_in(GO, &['語']).unwrap();
        assert_eq!(kanjidic.version.as_deref(), Some("2026-10-01"));
    }

    #[test]
    fn no_creation_date_gives_no_version() {
        let doc = format!("<kanjidic2>\n{GO}\n</kanjidic2>\n");
        let kanjidic = parse(
            source::open(Cursor::new(doc.into_bytes())).unwrap(),
            &BTreeSet::from(['語']),
        )
        .unwrap();
        assert_eq!(kanjidic.version, None);
        assert_eq!(kanjidic.entries.len(), 1);
    }

    #[test]
    fn reads_gzip_input() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(doc(GO).as_bytes()).unwrap();
        let gz = encoder.finish().unwrap();
        let kanjidic = parse(
            source::open(Cursor::new(gz)).unwrap(),
            &BTreeSet::from(['語']),
        )
        .unwrap();
        assert_eq!(kanjidic.entries.get(&'語'), Some(&go_expected()));
    }
}
