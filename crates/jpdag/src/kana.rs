//! Kana: the romaji conversion, the conversion between hiragana and katakana, and the morae of a
//! reading.

use std::sync::LazyLock;

const MOZC_TABLE: &str = include_str!("../data/mozc/romanji-hiragana.tsv");

/// Distance from a hiragana to its katakana.
const KATAKANA_OFFSET: u32 = 0x60;

/// Maps romajii -> kana
struct Row {
    input: &'static str,
    output: &'static str,
    /// Letters at the end of `input` that stay for the next row. `kk` gives っ, and its second
    /// `k` stays.
    pending: &'static str,
}

/// Rows of the table that give kana. The table also has rows for punctuation and symbols.
static ROWS: LazyLock<Vec<Row>> = LazyLock::new(|| {
    let rows = MOZC_TABLE.lines().filter_map(|line| {
        let mut columns = line.split('\t');
        let row = Row {
            input: columns.next()?,
            output: columns.next()?,
            pending: columns.next().unwrap_or(""),
        };
        let is_kana = row.output.chars().all(|c| matches!(c, 'ぁ'..='ゖ' | 'ー'));
        (is_kana && !row.output.is_empty()).then_some(row)
    });
    rows.collect()
});

/// Converts romaji to hiragana as an input method does, with the rows of [`ROWS`]. At each
/// position, the longest row that matches gives the kana. Characters that start no row stay as
/// they are.
///
/// The conversion drops an unfinished syllable at the end of `text`: letters that start a row and
/// do not complete it, because the user has not yet typed the rest. If any other letter is not
/// part of a row, the result is `None`.
pub fn romaji_to_hiragana(text: &str) -> Option<String> {
    let text = text.to_ascii_lowercase();
    let mut rest = text.as_str();
    let mut kana = String::new();
    while let Some(c) = rest.chars().next() {
        // `n` is the only row that also starts other rows. At the end of the text it gives ん.
        let is_row = ROWS.iter().any(|row| row.input == rest);
        if !is_row && ROWS.iter().any(|row| row.input.starts_with(rest)) {
            break;
        }
        let rows = ROWS.iter().filter(|row| rest.starts_with(row.input));
        if let Some(row) = rows.max_by_key(|row| row.input.len()) {
            kana.push_str(row.output);
            rest = &rest[row.input.len() - row.pending.len()..];
        } else if c.is_ascii_alphabetic() {
            return None;
        } else {
            kana.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    Some(kana)
}

/// Converts each katakana of `text` to hiragana.
pub fn to_hiragana(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'ァ'..='ヶ' | 'ヽ' | 'ヾ' => {
                char::from_u32(c as u32 - KATAKANA_OFFSET).unwrap_or(c)
            }
            _ => c,
        })
        .collect()
}

/// Converts each hiragana of `text` to katakana.
pub fn to_katakana(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'ぁ'..='ゖ' | 'ゝ' | 'ゞ' => {
                char::from_u32(c as u32 + KATAKANA_OFFSET).unwrap_or(c)
            }
            _ => c,
        })
        .collect()
}

/// Whether a character is kana: hiragana, katakana, or ー. The iteration marks ゝ, ゞ, ヽ, and ヾ
/// are kana too. ・ is not.
pub fn is_kana(c: char) -> bool {
    matches!(c, 'ぁ'..='ゖ' | 'ゝ' | 'ゞ' | 'ァ'..='ヺ' | 'ー' | 'ヽ' | 'ヾ')
}

/// Gives the number of characters of a text that are not kana: 1 for 鳥かご, and 2 for 鳥打ち.
pub fn length_without_kana(text: &str) -> usize {
    text.chars().filter(|&c| !is_kana(c)).count()
}

/// Splits a reading into its morae, the units of its rhythm and its pitch. A small ゃ, ゅ, ょ, ぁ,
/// ぃ, ぅ, ぇ, ぉ, or ゎ (or its katakana) joins the kana before it: きょう is きょ and う. A small っ,
/// ん, and ー are each a mora.
pub fn morae(text: &str) -> Vec<&str> {
    let mut morae: Vec<&str> = Vec::new();
    let mut start = 0;
    for (index, c) in text.char_indices() {
        let joins = matches!(
            c,
            'ゃ' | 'ゅ'
                | 'ょ'
                | 'ぁ'
                | 'ぃ'
                | 'ぅ'
                | 'ぇ'
                | 'ぉ'
                | 'ゎ'
                | 'ャ'
                | 'ュ'
                | 'ョ'
                | 'ァ'
                | 'ィ'
                | 'ゥ'
                | 'ェ'
                | 'ォ'
                | 'ヮ'
        );
        if index > 0 && !joins {
            morae.push(&text[start..index]);
            start = index;
        }
    }
    if start < text.len() {
        morae.push(&text[start..]);
    }
    morae
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hiragana(romaji: &str) -> String {
        romaji_to_hiragana(romaji).unwrap()
    }

    #[test]
    fn each_row_of_the_table_converts() {
        assert!(ROWS.len() > 300);
        for row in ROWS.iter() {
            assert!(row.input.is_ascii(), "{}", row.input);
            assert!(row.pending.len() < row.input.len(), "{}", row.input);
            assert!(row.input.ends_with(row.pending), "{}", row.input);
            assert_eq!(hiragana(row.input), row.output, "{}", row.input);
        }
    }

    #[test]
    fn romaji_reads_both_spellings() {
        assert_eq!(hiragana("shi"), "し");
        assert_eq!(hiragana("si"), "し");
        assert_eq!(hiragana("tsu"), hiragana("tu"));
        assert_eq!(hiragana("chi"), hiragana("ti"));
        assert_eq!(hiragana("fu"), hiragana("hu"));
        assert_eq!(hiragana("ji"), hiragana("zi"));
        assert_eq!(hiragana("kitte"), "きって");
        assert_eq!(hiragana("ko-hi-"), "こーひー");
    }

    #[test]
    fn romaji_ignores_case() {
        assert_eq!(hiragana("NeKo"), "ねこ");
    }

    #[test]
    fn romaji_reads_the_syllables_with_y() {
        assert_eq!(hiragana("kyou"), "きょう");
        assert_eq!(hiragana("ryokou"), "りょこう");
        assert_eq!(hiragana("jusho"), "じゅしょ");
        assert_eq!(hiragana("otya"), "おちゃ");
    }

    #[test]
    fn romaji_reads_nn_as_an_input_method_does() {
        assert_eq!(hiragana("onna"), "おんあ");
        assert_eq!(hiragana("onnna"), "おんな");
        assert_eq!(hiragana("kannji"), "かんじ");
        assert_eq!(hiragana("honnya"), "ほんや");
        assert_eq!(hiragana("kann"), "かん");
    }

    #[test]
    fn n_gives_the_syllabic_n() {
        assert_eq!(hiragana("kan"), "かん");
        assert_eq!(hiragana("kanji"), "かんじ");
        assert_eq!(hiragana("kan'i"), "かんい");
        assert_eq!(hiragana("kani"), "かに");
    }

    #[test]
    fn doubled_consonant_gives_the_small_tsu() {
        assert_eq!(hiragana("zasshi"), "ざっし");
        assert_eq!(hiragana("matcha"), "まっちゃ");
        assert_eq!(hiragana("gakk"), "がっ");
    }

    #[test]
    fn unfinished_syllable_is_dropped() {
        assert_eq!(hiragana("gak"), "が");
        assert_eq!(hiragana("g"), "");
        assert_eq!(hiragana("sh"), "");
        assert_eq!(hiragana("toky"), "と");
        assert_eq!(hiragana("kany"), "か");
    }

    #[test]
    fn letter_outside_a_row_gives_none() {
        assert_eq!(romaji_to_hiragana("xq"), None);
        assert_eq!(romaji_to_hiragana("gakso"), None);
        assert_eq!(romaji_to_hiragana("shimbun"), None);
    }

    #[test]
    fn other_characters_stay() {
        assert_eq!(hiragana("食べmasu"), "食べます");
        // The rows of the table for punctuation are not used.
        assert_eq!(hiragana("a.i"), "あ.い");
    }

    #[test]
    fn kana_do_not_count_in_the_length() {
        assert_eq!(length_without_kana("鳥かご"), 1);
        assert_eq!(length_without_kana("鳥打ち"), 2);
        assert_eq!(length_without_kana("Ｔシャツ"), 1);
        assert_eq!(length_without_kana("コーヒー"), 0);
        assert_eq!(length_without_kana("人々"), 2);
    }

    #[test]
    fn small_kana_join_the_kana_before() {
        assert_eq!(morae("きょう"), ["きょ", "う"]);
        assert_eq!(morae("がっこう"), ["が", "っ", "こ", "う"]);
        assert_eq!(morae("コーヒー"), ["コ", "ー", "ヒ", "ー"]);
        assert_eq!(morae("ほんや"), ["ほ", "ん", "や"]);
        assert_eq!(morae("ティッシュ"), ["ティ", "ッ", "シュ"]);
        assert_eq!(morae("ゃ"), ["ゃ"]);
        assert!(morae("").is_empty());
    }

    #[test]
    fn kana_convert_both_ways() {
        assert_eq!(to_hiragana("コーヒー"), "こーひー");
        assert_eq!(to_katakana("こーひー"), "コーヒー");
        assert_eq!(to_katakana("ねこ"), "ネコ");
        assert_eq!(to_hiragana("ヴ"), "ゔ");
        assert_eq!(to_hiragana("猫ノa"), "猫のa");
    }
}
