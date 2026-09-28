//! Stage 3: parse KanjiVG.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use jpdag::normalize::{normalize, normalize_str};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use crate::source::Source;

#[derive(Debug, Default)]
pub struct KanjiVg {
    /// The direct components of each KanjiVG entry, in the order of the groups.
    pub entries: HashMap<char, Vec<char>>,
    /// The groups whose `kvg:element` value does not give exactly 1 code point after normalization.
    pub unresolved: Vec<UnresolvedGroup>,
    /// The date in the header comment "This file was generated on YYYY-MM-DD".
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedGroup {
    /// The character of the KanjiVG entry that holds the group.
    pub kanji: char,
    /// The `kvg:element` value of the group.
    pub element: String,
}

/// A KanjiVG entry that the parser reads.
struct OpenEntry {
    kanji: char,
    components: Vec<char>,
    /// For each open group: true if the group or a group that holds it gives a direct component.
    groups: Vec<bool>,
}

impl OpenEntry {
    fn open_group(&mut self, group: &BytesStart, unresolved: &mut Vec<UnresolvedGroup>) {
        let Some(&stop) = self.groups.last() else {
            // The top group.
            self.groups.push(false);
            return;
        };
        if stop {
            self.groups.push(true);
            return;
        }
        let gives_component = match attribute(group, "kvg:element") {
            None => false,
            Some(element) => match single_char(&normalize_str(&element)) {
                Some(c) if c == self.kanji => false,
                Some(c) => {
                    if !self.components.contains(&c) {
                        self.components.push(c);
                    }
                    true
                }
                None => {
                    unresolved.push(UnresolvedGroup {
                        kanji: self.kanji,
                        element,
                    });
                    false
                }
            },
        };
        self.groups.push(gives_component);
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

/// The code point of a `<kanji>` element, for example U+8A9E for `kvg:kanji_08a9e`. An `id` with
/// a suffix after the code point, for example `kvg:kanji_08a9e-Kaisho`, gives no code point.
fn entry_code_point(id: &str) -> Option<char> {
    let hex = id.strip_prefix("kvg:kanji_")?;
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    char::from_u32(u32::from_str_radix(hex, 16).ok()?)
}

/// The code point of the KanjiVG entry of a `<kanji>` element, or `None` if stage 3 ignores the
/// element.
fn entry_kanji(element: &BytesStart) -> Option<char> {
    let c = entry_code_point(&attribute(element, "id")?)?;
    (normalize(c) == c.to_string()).then_some(c)
}

/// The date in "This file was generated on YYYY-MM-DD", if the comment holds it.
fn generated_date(comment: &str) -> Option<String> {
    const PREFIX: &str = "This file was generated on ";
    let rest = &comment[comment.find(PREFIX)? + PREFIX.len()..];
    let date: String = rest.chars().take(10).collect();
    let is_date = date.len() == 10
        && date.char_indices().all(|(i, c)| {
            if i == 4 || i == 7 {
                c == '-'
            } else {
                c.is_ascii_digit()
            }
        });
    is_date.then_some(date)
}

/// Parses the KanjiVG data of `input`.
pub fn parse(input: Source) -> Result<KanjiVg> {
    let mut reader = Reader::from_reader(input);
    let mut buf = Vec::new();
    let mut result = KanjiVg::default();

    let mut depth = 0usize;
    // The depth of the <kanji> element that stage 3 ignores.
    let mut ignored: Option<usize> = None;
    let mut entry: Option<OpenEntry> = None;
    let mut open_line = Vec::new();

    loop {
        let line = reader.get_ref().line();
        let event = reader
            .read_event_into(&mut buf)
            .map_err(|e| anyhow!("KanjiVG line {}: {e}", reader.get_ref().line()))?;
        match event {
            Event::Comment(e) => {
                if result.version.is_none() {
                    result.version = generated_date(e.as_ref());
                }
            }
            Event::Start(e) => {
                depth += 1;
                open_line.push((e.name().as_ref().to_owned(), line));
                if ignored.is_some() {
                    // Inside an ignored <kanji> element.
                } else if e.name().as_ref() == "kanji" {
                    match entry_kanji(&e) {
                        Some(kanji) => {
                            entry = Some(OpenEntry {
                                kanji,
                                components: Vec::new(),
                                groups: Vec::new(),
                            })
                        }
                        None => ignored = Some(depth),
                    }
                } else if e.name().as_ref() == "g"
                    && let Some(entry) = entry.as_mut()
                {
                    entry.open_group(&e, &mut result.unresolved);
                }
            }
            Event::Empty(e) => {
                if ignored.is_none() {
                    if e.name().as_ref() == "kanji" {
                        if let Some(kanji) = entry_kanji(&e) {
                            result.entries.insert(kanji, Vec::new());
                        }
                    } else if e.name().as_ref() == "g"
                        && let Some(entry) = entry.as_mut()
                    {
                        entry.open_group(&e, &mut result.unresolved);
                        entry.groups.pop();
                    }
                }
            }
            Event::End(e) => {
                open_line.pop();
                if ignored == Some(depth) {
                    ignored = None;
                } else if ignored.is_none() {
                    if e.name().as_ref() == "kanji" {
                        if let Some(done) = entry.take() {
                            result.entries.insert(done.kanji, done.components);
                        }
                    } else if e.name().as_ref() == "g"
                        && let Some(entry) = entry.as_mut()
                    {
                        entry.groups.pop();
                    }
                }
                depth -= 1;
            }
            Event::Eof => {
                if let Some((name, start_line)) = open_line.last() {
                    bail!("KanjiVG line {start_line}: element <{name}> is not closed");
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
    use std::io::Cursor;

    fn parse_str(body: &str) -> KanjiVg {
        let doc = format!("<kanjivg xmlns:kvg='http://kanjivg.tagaini.net'>\n{body}\n</kanjivg>\n");
        parse(source::open(Cursor::new(doc.into_bytes())).unwrap()).unwrap()
    }

    const GO: &str = r#"<kanji id="kvg:kanji_08a9e">
<g id="kvg:08a9e" kvg:element="語">
  <g id="kvg:08a9e-g1" kvg:element="言" kvg:position="left" kvg:radical="general">
    <g id="kvg:08a9e-g2" kvg:element="口"><path id="kvg:08a9e-s1" d="M0,0"/></g>
  </g>
  <g id="kvg:08a9e-g3" kvg:element="吾" kvg:position="right" kvg:phon="吾">
    <g id="kvg:08a9e-g4" kvg:element="五">
      <g id="kvg:08a9e-g5" kvg:element="二" kvg:part="1"/>
      <g id="kvg:08a9e-g6" kvg:element="二" kvg:part="2"/>
    </g>
    <g id="kvg:08a9e-g7" kvg:element="口"/>
  </g>
</g>
</kanji>"#;

    fn components(kvg: &KanjiVg, kanji: char) -> Vec<char> {
        kvg.entries
            .get(&kanji)
            .cloned()
            .unwrap_or_else(|| panic!("no entry for {kanji}"))
    }

    fn entry(code: &str, top: &str, groups: &str) -> String {
        format!(
            r#"<kanji id="kvg:kanji_{code}"><g id="kvg:{code}" kvg:element="{top}">{groups}</g></kanji>"#
        )
    }

    #[test]
    fn records_direct_components_only() {
        assert_eq!(components(&parse_str(GO), '語'), vec!['言', '吾']);
    }

    #[test]
    fn group_without_element_passes_to_its_children() {
        let kvg = parse_str(&entry(
            "04f11",
            "休",
            r#"<g><g kvg:element="亻"/><g kvg:element="木"/></g>"#,
        ));
        assert_eq!(components(&kvg, '休'), vec!['亻', '木']);
    }

    #[test]
    fn split_component_gives_one_component() {
        let kvg = parse_str(&entry(
            "056de",
            "回",
            r#"<g kvg:element="囗" kvg:part="1"/><g kvg:element="口"/><g kvg:element="囗" kvg:part="2"/>"#,
        ));
        assert_eq!(components(&kvg, '回'), vec!['囗', '口']);
    }

    #[test]
    fn character_is_not_its_own_component() {
        let kvg = parse_str(&entry("053e3", "口", r#"<path d="M0,0"/>"#));
        assert_eq!(components(&kvg, '口'), Vec::<char>::new());
    }

    #[test]
    fn variant_style_entry_is_ignored() {
        let variant = GO.replace("kvg:kanji_08a9e", "kvg:kanji_08a9e-Kaisho");
        assert!(parse_str(&variant).entries.is_empty());
    }

    #[test]
    fn uses_element_not_original() {
        let kvg = parse_str(&entry(
            "04f11",
            "休",
            r#"<g kvg:element="亻" kvg:original="人"/>"#,
        ));
        assert_eq!(components(&kvg, '休'), vec!['亻']);
    }

    #[test]
    fn element_is_normalized() {
        let kvg = parse_str(&entry("0540d", "名", "<g kvg:element=\"\u{2F1D}\"/>"));
        assert_eq!(components(&kvg, '名'), vec!['\u{53E3}']);
    }

    #[test]
    fn entry_of_a_supplementary_plane() {
        let kvg = parse_str(&entry("20b9f", "\u{20B9F}", r#"<g kvg:element="口"/>"#));
        assert!(kvg.entries.contains_key(&'\u{20B9F}'));
    }

    #[test]
    fn cdp_code_is_reported_and_its_children_visited() {
        let kvg = parse_str(&entry(
            "04e80",
            "亀",
            r#"<g kvg:element="CDP-8BD0"><g kvg:element="田"/></g>"#,
        ));
        assert_eq!(components(&kvg, '亀'), vec!['田']);
        assert_eq!(
            kvg.unresolved,
            vec![UnresolvedGroup {
                kanji: '亀',
                element: "CDP-8BD0".into()
            }]
        );
    }

    #[test]
    fn nested_group_that_names_the_entry_is_skipped() {
        // ⼝ (U+2F1D) normalizes to 口, the kanji of the entry.
        let kvg = parse_str(&entry(
            "053e3",
            "口",
            "<g kvg:element=\"\u{2F1D}\"><g kvg:element=\"丨\"/><g kvg:element=\"一\"/></g>",
        ));
        assert_eq!(components(&kvg, '口'), vec!['丨', '一']);
    }

    #[test]
    fn radical_eat_two_is_a_component() {
        let kvg = parse_str(&entry(
            "098ef",
            "飯",
            r#"<g kvg:element="⻞"/><g kvg:element="反"/>"#,
        ));
        assert_eq!(components(&kvg, '飯'), vec!['⻞', '反']);
    }

    #[test]
    fn generated_comment_gives_the_version() {
        let doc = "<!--\nCopyright (C) 2009-2013 Ulrich Apel.\nThis file was generated on 2025-08-16 \
                   from the most recent KanjiVG data.\n-->\n<kanjivg xmlns:kvg='http://kanjivg.tagaini.net'>\n</kanjivg>\n";
        let kvg = parse(source::open(Cursor::new(doc.as_bytes().to_vec())).unwrap()).unwrap();
        assert_eq!(kvg.version.as_deref(), Some("2025-08-16"));
        assert_eq!(parse_str("").version, None);
    }

    #[test]
    fn entry_that_normalization_changes_is_ignored() {
        let kvg = parse_str(&entry("02ea8", "⺨", r#"<g kvg:element="丿"/>"#));
        assert!(kvg.entries.is_empty());
    }
}
