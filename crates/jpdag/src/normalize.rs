//! Normalization: NFKC, then the merge of radical code points that look identical to a unified
//! ideograph.

use unicode_normalization::UnicodeNormalization;

/// Code points that NFKC does not merge with the unified ideograph that looks identical.
///
/// ⻖ (U+2ED6) and ⻏ (U+2ECF) are not in the table. They look identical to 阝, and to each other,
/// but they are 2 characters: ⻖ is the left form of 阜, and ⻏ is the right form of 邑.
const MERGE: [(char, char); 3] = [
    ('\u{2EA8}', '\u{72AD}'), // ⺨ → 犭
    ('\u{2E89}', '\u{5202}'), // ⺉ → 刂
    ('\u{2EA1}', '\u{6C35}'), // ⺡ → 氵
];

/// Normalizes 1 code point. The result has 1 or more code points, for example 平成 for ㍻.
pub fn normalize(c: char) -> String {
    let nfkc: String = std::iter::once(c).nfkc().collect();
    let mut chars = nfkc.chars();
    if let (Some(single), None) = (chars.next(), chars.next())
        && let Some(&(_, to)) = MERGE.iter().find(|&&(from, _)| from == single)
    {
        return to.to_string();
    }
    nfkc
}

/// Normalizes each code point of `s`, in order.
pub fn normalize_str(s: &str) -> String {
    s.chars().map(normalize).collect()
}

/// Normalizes `c`, and gives the result if the result is exactly 1 code point.
pub fn normalize_char(c: char) -> Option<char> {
    let normalized = normalize(c);
    let mut chars = normalized.chars();
    match (chars.next(), chars.next()) {
        (Some(single), None) => Some(single),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn kangxi_radical_becomes_unified_ideograph() {
        // ⼝ → 口
        assert_eq!(normalize('\u{2F1D}'), "\u{53E3}");
    }

    #[test]
    fn compatibility_ideograph_becomes_unified_ideograph() {
        // 塚 → 塚
        assert_eq!(normalize('\u{FA10}'), "\u{585A}");
    }

    #[test]
    fn radical_eat_two_is_unchanged() {
        // ⻞ → ⻞
        assert_eq!(normalize('\u{2EDE}'), "\u{2EDE}");
    }

    #[test]
    fn square_era_name_gives_two_code_points() {
        // ㍻ → 平成
        assert_eq!(normalize('\u{337B}'), "平成");
    }

    #[test]
    fn radical_simplified_walk_is_unchanged() {
        // ⻌ → ⻌
        assert_eq!(normalize('\u{2ECC}'), "\u{2ECC}");
    }

    #[test]
    fn person_radical_ideograph_is_unchanged() {
        // 亻 → 亻
        assert_eq!(normalize('\u{4EBB}'), "\u{4EBB}");
    }

    #[test]
    fn radical_mound_and_radical_city_are_unchanged() {
        // ⻖ → ⻖, and ⻏ → ⻏
        assert_eq!(normalize('\u{2ED6}'), "\u{2ED6}");
        assert_eq!(normalize('\u{2ECF}'), "\u{2ECF}");
    }

    #[test]
    fn radical_dog_merges_with_its_ideograph() {
        // ⺨ → 犭
        assert_eq!(normalize('\u{2EA8}'), "\u{72AD}");
    }

    #[test]
    fn radical_small_is_unchanged() {
        // ⺌ → ⺌
        assert_eq!(normalize('\u{2E8C}'), "\u{2E8C}");
    }
}
