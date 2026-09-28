//! Stage 5: complete the character set.

use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Default)]
pub struct CharacterSet {
    /// The characters of the `character` rows.
    pub characters: BTreeSet<char>,
    /// The `character_component` rows, each as (character, direct component).
    pub character_components: BTreeSet<(char, char)>,
    /// The `mutant` rows, each as (mutant, base).
    pub mutants: BTreeSet<(char, char)>,
    /// The number of characters with a KanjiVG entry.
    pub with_entry: usize,
    /// The kanji of a word with no KanjiVG entry.
    pub uncovered_word_kanji: BTreeSet<char>,
    /// The direct components with no KanjiVG entry.
    pub uncovered_components: BTreeSet<char>,
}

impl CharacterSet {
    /// The number of uncovered characters: the characters with no KanjiVG entry.
    pub fn uncovered(&self) -> usize {
        self.characters.len() - self.with_entry
    }
}

/// Completes the character set from the kanji of the stage 2 rows, the direct components of each
/// KanjiVG entry, and the stage 4 rows.
pub fn complete(
    word_kanji: impl IntoIterator<Item = char>,
    kanjivg: &HashMap<char, Vec<char>>,
    stage4: &[(char, char)],
) -> CharacterSet {
    let mut bases: HashMap<char, Vec<char>> = HashMap::new();
    for &(mutant, base) in stage4 {
        bases.entry(mutant).or_default().push(base);
    }

    let mut set = CharacterSet::default();
    let word_kanji: BTreeSet<char> = word_kanji.into_iter().collect();
    // Step 1.
    let mut unexamined: Vec<char> = word_kanji.iter().copied().collect();
    set.characters.extend(&word_kanji);

    // Steps 2 to 5.
    while let Some(c) = unexamined.pop() {
        let mut add = |d: char| {
            if set.characters.insert(d) {
                unexamined.push(d);
            }
        };
        if let Some(components) = kanjivg.get(&c) {
            set.with_entry += 1;
            for &component in components {
                set.character_components.insert((c, component));
                add(component);
            }
        } else if word_kanji.contains(&c) {
            set.uncovered_word_kanji.insert(c);
        }
        for &base in bases.get(&c).into_iter().flatten() {
            set.mutants.insert((c, base));
            add(base);
        }
    }

    set.uncovered_components = set
        .character_components
        .iter()
        .map(|&(_, component)| component)
        .filter(|component| !kanjivg.contains_key(component))
        .collect();
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kanjivg(entries: &[(char, &[char])]) -> HashMap<char, Vec<char>> {
        entries
            .iter()
            .map(|&(c, components)| (c, components.to_vec()))
            .collect()
    }

    #[test]
    fn kanji_with_an_entry_gets_its_components() {
        let set = complete(
            ['語'],
            &kanjivg(&[('語', &['言', '吾']), ('言', &[]), ('吾', &[])]),
            &[],
        );
        assert_eq!(
            set.character_components,
            BTreeSet::from([('語', '言'), ('語', '吾')])
        );
        assert_eq!(set.characters, BTreeSet::from(['語', '言', '吾']));
    }

    #[test]
    fn component_without_an_entry_is_uncovered() {
        let set = complete(['每'], &kanjivg(&[('每', &['𠂉', '母']), ('母', &[])]), &[]);
        assert!(set.characters.contains(&'𠂉'));
        assert!(!set.character_components.iter().any(|&(c, _)| c == '𠂉'));
        assert_eq!(set.uncovered_components, BTreeSet::from(['𠂉']));
        assert_eq!(set.uncovered(), 1);
    }

    #[test]
    fn kanji_without_an_entry_is_uncovered() {
        let set = complete(['𠮟'], &kanjivg(&[]), &[]);
        assert_eq!(set.characters, BTreeSet::from(['𠮟']));
        assert_eq!(set.uncovered_word_kanji, BTreeSet::from(['𠮟']));
    }

    #[test]
    fn components_of_a_component_join_the_set() {
        let set = complete(
            ['語'],
            &kanjivg(&[('語', &['言', '吾']), ('吾', &['五', '口']), ('言', &[])]),
            &[],
        );
        assert!(set.character_components.contains(&('吾', '五')));
        assert!(set.characters.contains(&'五') && set.characters.contains(&'口'));
    }

    #[test]
    fn base_of_a_mutant_joins_the_set() {
        let set = complete(['休'], &kanjivg(&[('休', &['亻', '木'])]), &[('亻', '人')]);
        assert!(set.characters.contains(&'人'));
        assert_eq!(set.mutants, BTreeSet::from([('亻', '人')]));
    }

    #[test]
    fn mutant_outside_the_set_gives_no_row() {
        let set = complete(['休'], &kanjivg(&[('休', &['亻', '木'])]), &[('氵', '水')]);
        assert!(set.mutants.is_empty());
        assert!(!set.characters.contains(&'水'));
    }
}
