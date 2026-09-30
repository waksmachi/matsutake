//! Stage 2: link each kanji form to its kanji.

use jpdag::normalize::normalize;

use crate::words::KanjiFormRow;

/// Code points with the Unicode property `Unified_Ideograph`, from `PropList-18.0.0.txt`.
const UNIFIED_IDEOGRAPH: [(u32, u32); 16] = [
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xFA0E, 0xFA0F),
    (0xFA11, 0xFA11),
    (0xFA13, 0xFA14),
    (0xFA1F, 0xFA1F),
    (0xFA21, 0xFA21),
    (0xFA23, 0xFA24),
    (0xFA27, 0xFA29),
    (0x20000, 0x2A6DF),
    (0x2A700, 0x2B81E),
    (0x2B820, 0x2CEAD),
    (0x2CEB0, 0x2EBE0),
    (0x2EBF0, 0x2EE5D),
    (0x30000, 0x3134A),
    (0x31350, 0x33479),
];

pub fn is_unified_ideograph(c: char) -> bool {
    let c = c as u32;
    UNIFIED_IDEOGRAPH
        .iter()
        .any(|&(first, last)| (first..=last).contains(&c))
}

/// Kanji of `text`, in text order: each normalized code point with the property
/// `Unified_Ideograph`, without duplicates.
pub fn kanji_of(text: &str) -> Vec<char> {
    let mut kanji = Vec::new();
    for c in text
        .chars()
        .flat_map(|c| normalize(c).chars().collect::<Vec<_>>())
    {
        if is_unified_ideograph(c) && !kanji.contains(&c) {
            kanji.push(c);
        }
    }
    kanji
}

/// `kanji_form_kanji` rows, each as (kanji_form_id, kanji). Tags and `uk` do not filter the
/// links.
pub fn link(kanji_forms: &[KanjiFormRow]) -> Vec<(u32, char)> {
    kanji_forms
        .iter()
        .flat_map(|k| kanji_of(&k.text).into_iter().map(move |c| (k.id, c)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jmdict::{Category, Entry, Jmdict, KanjiForm, Sense};
    use crate::words;

    fn links(forms: &[(&str, &[&str])], sense_tags: &[(Category, &str)]) -> Vec<(u32, char)> {
        let entry = Entry {
            seq: 1,
            kanji: forms
                .iter()
                .map(|(text, tags)| KanjiForm {
                    text: (*text).into(),
                    tags: tags.iter().map(|t| (*t).into()).collect(),
                })
                .collect(),
            senses: vec![Sense {
                tags: sense_tags.iter().map(|(c, t)| (*c, (*t).into())).collect(),
                ..Sense::default()
            }],
            ..Entry::default()
        };
        let jmdict = Jmdict {
            entries: vec![entry],
            ..Jmdict::default()
        };
        link(&words::build(&jmdict).kanji_forms)
    }

    #[test]
    fn each_kanji_form_links_to_its_kanji() {
        assert_eq!(
            links(&[("一寸", &[]), ("鳥渡", &[])], &[]),
            vec![(1, '一'), (1, '寸'), (2, '鳥'), (2, '渡')]
        );
    }

    #[test]
    fn tags_do_not_filter_the_links() {
        assert_eq!(links(&[("鳥渡", &["rK"])], &[]), vec![(1, '鳥'), (1, '渡')]);
    }

    #[test]
    fn uk_does_not_filter_the_links() {
        assert_eq!(
            links(&[("一寸", &[])], &[(Category::Misc, "uk")]),
            vec![(1, '一'), (1, '寸')]
        );
    }

    #[test]
    fn iteration_mark_is_not_a_kanji() {
        assert_eq!(kanji_of("人々"), vec!['人']);
    }

    #[test]
    fn small_ke_is_not_a_kanji() {
        assert_eq!(kanji_of("一ヶ月"), vec!['一', '月']);
    }

    #[test]
    fn okurigana_are_not_kanji() {
        assert_eq!(kanji_of("取り扱い"), vec!['取', '扱']);
    }

    #[test]
    fn form_without_kanji_gives_no_links() {
        assert!(kanji_of("Tシャツ").is_empty());
    }

    #[test]
    fn shime_mark_is_not_a_kanji() {
        assert_eq!(kanji_of("〆切"), vec!['切']);
    }

    #[test]
    fn kanji_of_a_supplementary_plane() {
        assert_eq!(kanji_of("\u{20B9F}る"), vec!['\u{20B9F}']);
    }

    #[test]
    fn compatibility_ideograph_links_its_unified_ideograph() {
        assert_eq!(kanji_of("\u{FA19}"), vec!['\u{795E}']);
    }

    #[test]
    fn unified_ideograph_in_the_compatibility_block_is_kept() {
        assert_eq!(kanji_of("\u{FA11}"), vec!['\u{FA11}']);
    }

    #[test]
    fn repeated_kanji_links_once() {
        assert_eq!(kanji_of("日日"), vec!['日']);
    }
}
