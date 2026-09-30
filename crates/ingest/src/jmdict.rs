//! Stage 1: parse JMdict.

use std::collections::HashMap;

use anyhow::{Context, Result, anyhow, bail};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::source::Source;

/// Category of a tag: the element that holds the tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// `<ke_inf>`: information about a kanji form, for example `ateji` (phonetic kanji), `iK`
    /// (irregular kanji), or `rK` (rare kanji form).
    KeInf,
    /// `<re_inf>`: information about a reading, for example `gikun` (the reading of a meaning),
    /// `ik` (irregular kana), or `ok` (out-dated kana).
    ReInf,
    /// `<pos>`: the part of speech of a sense, for example `n` (noun), `adj-i` (i-adjective), or
    /// `v5r` (godan verb that ends in る).
    Pos,
    /// `<field>`: the field that a sense belongs to, for example `med` (medicine) or `comp`
    /// (computing).
    Field,
    /// `<misc>`: other information about a sense, for example `uk` (usually written in kana),
    /// `col` (colloquial), or `hon` (honorific).
    Misc,
    /// `<dial>`: the dialect of a sense, for example `ksb` (Kansai-ben).
    Dial,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::KeInf => "ke_inf",
            Category::ReInf => "re_inf",
            Category::Pos => "pos",
            Category::Field => "field",
            Category::Misc => "misc",
            Category::Dial => "dial",
        }
    }

    /// Category of a tag that a `<sense>` element holds.
    fn of_sense_element(name: &str) -> Option<Self> {
        match name {
            "pos" => Some(Category::Pos),
            "field" => Some(Category::Field),
            "misc" => Some(Category::Misc),
            "dial" => Some(Category::Dial),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct Jmdict {
    pub entries: Vec<Entry>,
    /// Text of each entity that the DTD declares, by the name of the entity.
    pub entities: HashMap<String, String>,
    /// Date in the comment `<!-- JMdict created: YYYY-MM-DD -->`.
    pub version: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `<ent_seq>` value.
    pub seq: u32,
    /// Kanji forms, in entry order.
    pub kanji: Vec<KanjiForm>,
    /// Readings, in entry order.
    pub readings: Vec<Reading>,
    /// Senses, in entry order.
    pub senses: Vec<Sense>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KanjiForm {
    /// `<keb>` text.
    pub text: String,
    /// `<ke_inf>` tags.
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reading {
    /// `<reb>` text.
    pub text: String,
    /// True if the element holds `<re_nokanji/>`.
    pub no_kanji: bool,
    /// `<re_restr>` texts: the kanji forms that the reading applies to.
    pub restrictions: Vec<String>,
    /// `<re_inf>` tags.
    pub tags: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sense {
    /// `<stagk>` texts: the kanji forms that the sense applies to.
    pub kanji_restrictions: Vec<String>,
    /// `<stagr>` texts: the readings that the sense applies to.
    pub reading_restrictions: Vec<String>,
    /// `<pos>`, `<field>`, `<misc>`, and `<dial>` tags, in sense order.
    pub tags: Vec<(Category, String)>,
    /// Glosses, in sense order.
    pub glosses: Vec<Gloss>,
    /// `<s_inf>` texts.
    pub notes: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Gloss {
    pub text: String,
    /// `g_type` value, for example `lit`.
    pub g_type: Option<String>,
}

/// Parses the JMdict data of `input`.
pub fn parse(input: Source) -> Result<Jmdict> {
    let mut reader = Reader::from_reader(input);
    let mut buf = Vec::new();
    let mut result = Jmdict::default();

    // The open elements, each with the line of its start tag.
    let mut open: Vec<(String, u64)> = Vec::new();
    let mut entry: Option<Entry> = None;
    // The text and the entity reference inside the innermost element.
    let mut text = String::new();
    let mut entity: Option<String> = None;
    // The `g_type` value of the open `<gloss>` element.
    let mut g_type: Option<String> = None;

    loop {
        let line = reader.get_ref().line();
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| anyhow!("JMdict line {}: {e}", reader.get_ref().line()))?;
        match event {
            Event::DocType(e) => result.entities = parse_entities(&e.into_inner()),
            Event::Comment(e) => {
                if result.version.is_none()
                    && let Some(date) = e.as_ref().trim().strip_prefix("JMdict created:")
                {
                    result.version = Some(date.trim().to_owned());
                }
            }
            Event::Start(e) => {
                let name = e.name().as_ref().to_owned();
                if let Some(entry) = entry.as_mut() {
                    open_element(entry, &name);
                } else if name == "entry" {
                    entry = Some(Entry::default());
                }
                if name == "gloss" {
                    g_type = attribute(&e, "g_type");
                }
                open.push((name, line));
                text.clear();
                entity = None;
            }
            Event::Empty(e) => {
                if let Some(entry) = entry.as_mut() {
                    match e.name().as_ref() {
                        "re_nokanji" => {
                            if let Some(r) = entry.readings.last_mut() {
                                r.no_kanji = true;
                            }
                        }
                        name => open_element(entry, name),
                    }
                }
            }
            Event::Text(t) => text.push_str(&t.xml10_content()),
            Event::CData(t) => text.push_str(&t.into_inner()),
            Event::GeneralRef(r) => {
                if r.is_char_ref() {
                    if let Some(c) = r
                        .resolve_char_ref()
                        .map_err(|e| anyhow!("JMdict line {line}: {e}"))?
                    {
                        text.push(c);
                    }
                } else if let Some(value) = resolve_predefined_entity(&r) {
                    text.push_str(value);
                } else {
                    // The tag is the name of the entity, not the text that the DTD gives.
                    entity = Some(r.into_inner().into_owned());
                }
            }
            Event::End(_) => {
                let (name, start_line) = open.pop().expect("quick-xml checks the end names");
                let value = text.trim();
                let tag = || entity.clone().unwrap_or_else(|| value.to_owned());
                if let Some(e) = entry.as_mut() {
                    match name.as_str() {
                        "ent_seq" => {
                            e.seq = value.parse().with_context(|| {
                                format!("JMdict line {start_line}: bad ent_seq {value:?}")
                            })?
                        }
                        "keb" => {
                            if let Some(k) = e.kanji.last_mut() {
                                k.text = value.to_owned();
                            }
                        }
                        "ke_inf" => {
                            if let Some(k) = e.kanji.last_mut() {
                                k.tags.push(tag());
                            }
                        }
                        "reb" => {
                            if let Some(r) = e.readings.last_mut() {
                                r.text = value.to_owned();
                            }
                        }
                        "re_inf" => {
                            if let Some(r) = e.readings.last_mut() {
                                r.tags.push(tag());
                            }
                        }
                        "re_restr" => {
                            if let Some(r) = e.readings.last_mut() {
                                r.restrictions.push(value.to_owned());
                            }
                        }
                        "stagk" => {
                            if let Some(s) = e.senses.last_mut() {
                                s.kanji_restrictions.push(value.to_owned());
                            }
                        }
                        "stagr" => {
                            if let Some(s) = e.senses.last_mut() {
                                s.reading_restrictions.push(value.to_owned());
                            }
                        }
                        "gloss" => {
                            if let Some(s) = e.senses.last_mut() {
                                s.glosses.push(Gloss {
                                    text: value.to_owned(),
                                    g_type: g_type.take(),
                                });
                            }
                        }
                        "s_inf" => {
                            if let Some(s) = e.senses.last_mut() {
                                s.notes.push(value.to_owned());
                            }
                        }
                        "entry" => {
                            if e.seq == 0 {
                                bail!("JMdict line {start_line}: entry has no ent_seq");
                            }
                            result.entries.push(entry.take().expect("inside an entry"));
                        }
                        other => {
                            if let Some(category) = Category::of_sense_element(other)
                                && let Some(s) = e.senses.last_mut()
                            {
                                s.tags.push((category, tag()));
                            }
                        }
                    }
                }
                text.clear();
                entity = None;
            }
            Event::Eof => {
                if let Some((name, start_line)) = open.last() {
                    bail!("JMdict line {start_line}: element <{name}> is not closed");
                }
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(result)
}

/// Adds the kanji form, reading, or sense that the element `name` starts.
fn open_element(entry: &mut Entry, name: &str) {
    match name {
        "k_ele" => entry.kanji.push(KanjiForm::default()),
        "r_ele" => entry.readings.push(Reading::default()),
        "sense" => entry.senses.push(Sense::default()),
        _ => {}
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

/// Reads the `<!ENTITY name "text">` declarations of a DTD.
fn parse_entities(dtd: &str) -> HashMap<String, String> {
    let mut entities = HashMap::new();
    let mut rest = dtd;
    while let Some(i) = rest.find("<!ENTITY") {
        let decl = rest[i + "<!ENTITY".len()..].trim_start();
        let Some(name_end) = decl.find(char::is_whitespace) else {
            break;
        };
        let (name, after) = (&decl[..name_end], decl[name_end..].trim_start());
        rest = after;
        let Some(quote) = after.chars().next().filter(|&c| c == '"' || c == '\'') else {
            continue;
        };
        let body = &after[1..];
        let Some(end) = body.find(quote) else {
            break;
        };
        entities.insert(name.to_owned(), body[..end].to_owned());
        rest = &body[end..];
    }
    entities
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source;
    use std::io::{Cursor, Write};

    const DTD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE JMdict [
<!ENTITY uk "word usually written using kana alone">
<!ENTITY iK "word containing irregular kanji usage">
<!ENTITY ateji "ateji (phonetic) reading">
<!ENTITY ik "word containing irregular kana usage">
<!ENTITY n "noun (common) (futsuumeishi)">
<!ENTITY adv "adverb (fukushi)">
<!ENTITY med "medicine">
<!ENTITY ksb "Kansai-ben">
]>
"#;

    fn doc(body: &str) -> String {
        format!("{DTD}<JMdict>\n{body}\n</JMdict>\n")
    }

    fn parse_str(body: &str) -> Result<Jmdict> {
        parse(source::open(Cursor::new(doc(body).into_bytes())).unwrap())
    }

    fn entries(body: &str) -> Vec<Entry> {
        parse_str(body).unwrap().entries
    }

    const CHOTTO: &str = "<entry>
<ent_seq>1234560</ent_seq>
<k_ele><keb>一寸</keb><ke_inf>&ateji;</ke_inf></k_ele>
<k_ele><keb>鳥渡</keb><ke_inf>&ateji;</ke_inf><ke_inf>&iK;</ke_inf></k_ele>
<r_ele><reb>ちょっと</reb></r_ele>
<sense><pos>&adv;</pos><gloss>a little</gloss></sense>
</entry>";

    fn chotto_expected() -> Entry {
        Entry {
            seq: 1234560,
            kanji: vec![
                KanjiForm {
                    text: "一寸".into(),
                    tags: vec!["ateji".into()],
                },
                KanjiForm {
                    text: "鳥渡".into(),
                    tags: vec!["ateji".into(), "iK".into()],
                },
            ],
            readings: vec![Reading {
                text: "ちょっと".into(),
                ..Reading::default()
            }],
            senses: vec![Sense {
                tags: vec![(Category::Pos, "adv".into())],
                glosses: vec![Gloss {
                    text: "a little".into(),
                    g_type: None,
                }],
                ..Sense::default()
            }],
        }
    }

    #[test]
    fn keeps_the_parts_of_an_entry_in_order() {
        assert_eq!(entries(CHOTTO), vec![chotto_expected()]);
    }

    #[test]
    fn entity_reference_gives_its_name_as_the_tag() {
        let e = entries(
            "<entry><ent_seq>1</ent_seq><k_ele><keb>一寸</keb></k_ele>
<sense><misc>&uk;</misc></sense></entry>",
        );
        assert_eq!(e[0].senses[0].tags, vec![(Category::Misc, "uk".to_owned())]);
    }

    #[test]
    fn entry_without_kanji_forms() {
        let e = entries(
            "<entry><ent_seq>2</ent_seq><r_ele><reb>ちょっと</reb></r_ele><sense/></entry>",
        );
        assert_eq!(e[0].seq, 2);
        assert!(e[0].kanji.is_empty());
        assert_eq!(e[0].senses.len(), 1);
    }

    #[test]
    fn tag_belongs_to_its_own_sense() {
        let e = entries(
            "<entry><ent_seq>3</ent_seq><k_ele><keb>一寸</keb></k_ele>
<sense><gloss>a</gloss></sense><sense><misc>&uk;</misc></sense></entry>",
        );
        assert!(e[0].senses[0].tags.is_empty());
        assert_eq!(e[0].senses[1].tags, vec![(Category::Misc, "uk".to_owned())]);
    }

    #[test]
    fn reads_gzip_input() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(doc(CHOTTO).as_bytes()).unwrap();
        let gz = encoder.finish().unwrap();
        let jmdict = parse(source::open(Cursor::new(gz)).unwrap()).unwrap();
        assert_eq!(jmdict.entries, vec![chotto_expected()]);
    }

    #[test]
    fn unclosed_element_error_gives_the_line() {
        // The <k_ele> element is on line 3 of the body.
        let body = "<entry>\n<ent_seq>4</ent_seq>\n<k_ele><keb>一寸</keb>\n</entry>";
        let error = parse_str(body).unwrap_err().to_string();
        let line = DTD.lines().count() + 1 + 4;
        assert!(error.contains(&format!("line {line}")), "{error}");

        let error = parse(source::open(Cursor::new(b"<JMdict>\n<entry>\n".to_vec())).unwrap())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("line 2") && error.contains("<entry>"),
            "{error}"
        );
    }

    #[test]
    fn undeclared_entity_keeps_its_name() {
        let jmdict = parse_str(
            "<entry><ent_seq>5</ent_seq><k_ele><keb>一寸</keb><ke_inf>&newtag;</ke_inf></k_ele></entry>",
        )
        .unwrap();
        assert_eq!(jmdict.entries[0].kanji[0].tags, vec!["newtag".to_owned()]);
        assert!(!jmdict.entities.contains_key("newtag"));
    }

    #[test]
    fn readings_keep_restrictions_no_kanji_and_tags() {
        let e = entries(
            "<entry><ent_seq>6</ent_seq><k_ele><keb>一寸</keb></k_ele>
<r_ele><reb>ちょっと</reb><re_restr>一寸</re_restr></r_ele>
<r_ele><reb>チョット</reb><re_nokanji/></r_ele>
<r_ele><reb>ちょつと</reb><re_inf>&ik;</re_inf></r_ele></entry>",
        );
        assert_eq!(
            e[0].readings,
            vec![
                Reading {
                    text: "ちょっと".into(),
                    restrictions: vec!["一寸".into()],
                    ..Reading::default()
                },
                Reading {
                    text: "チョット".into(),
                    no_kanji: true,
                    ..Reading::default()
                },
                Reading {
                    text: "ちょつと".into(),
                    tags: vec!["ik".into()],
                    ..Reading::default()
                },
            ]
        );
    }

    #[test]
    fn sense_keeps_restrictions_and_tags_by_category() {
        let e = entries(
            "<entry><ent_seq>7</ent_seq><k_ele><keb>一寸</keb></k_ele><r_ele><reb>ちょっと</reb></r_ele>
<sense><stagk>一寸</stagk><stagr>ちょっと</stagr><pos>&n;</pos><field>&med;</field>
<misc>&uk;</misc><dial>&ksb;</dial><gloss>x</gloss></sense></entry>",
        );
        let sense = &e[0].senses[0];
        assert_eq!(sense.kanji_restrictions, vec!["一寸".to_owned()]);
        assert_eq!(sense.reading_restrictions, vec!["ちょっと".to_owned()]);
        assert_eq!(
            sense.tags,
            vec![
                (Category::Pos, "n".to_owned()),
                (Category::Field, "med".to_owned()),
                (Category::Misc, "uk".to_owned()),
                (Category::Dial, "ksb".to_owned()),
            ]
        );
    }

    #[test]
    fn dtd_gives_the_entity_descriptions() {
        let jmdict = parse_str("").unwrap();
        assert_eq!(
            jmdict.entities.get("uk").map(String::as_str),
            Some("word usually written using kana alone")
        );
        assert_eq!(jmdict.entities.len(), 8);
    }

    #[test]
    fn glosses_keep_their_order_and_type() {
        let e = entries(
            r#"<entry><ent_seq>8</ent_seq><r_ele><reb>て</reb></r_ele>
<sense><gloss>hand</gloss><gloss g_type="lit">arm</gloss></sense></entry>"#,
        );
        assert_eq!(
            e[0].senses[0].glosses,
            vec![
                Gloss {
                    text: "hand".into(),
                    g_type: None,
                },
                Gloss {
                    text: "arm".into(),
                    g_type: Some("lit".into()),
                },
            ]
        );
    }

    #[test]
    fn sense_keeps_its_note() {
        let e = entries(
            "<entry><ent_seq>9</ent_seq><r_ele><reb>ちょっと</reb></r_ele>
<sense><s_inf>before a verb in negative form</s_inf><gloss>(not) easily</gloss></sense></entry>",
        );
        assert_eq!(
            e[0].senses[0].notes,
            vec!["before a verb in negative form".to_owned()]
        );
    }

    #[test]
    fn creation_comment_gives_the_version() {
        let jmdict = parse_str("<!-- JMdict created: 2026-09-28 -->").unwrap();
        assert_eq!(jmdict.version.as_deref(), Some("2026-09-28"));
    }

    #[test]
    fn no_creation_comment_gives_no_version() {
        assert_eq!(parse_str("").unwrap().version, None);
    }
}
