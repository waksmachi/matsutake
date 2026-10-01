//! Stage 4: read the mutants.

use std::collections::HashSet;

use anyhow::{Result, bail};
use jpdag::normalize::normalize_char;

/// Rows of `data/mutants.tsv`.
pub const MUTANTS: &str = include_str!("../data/mutants.tsv");

/// Reads a list of mutants, and gives each row as (mutant, base) after normalization (steps 1 to 5
/// and 7).
pub fn read(text: &str) -> Result<Vec<(char, char)>> {
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        let (Some(mutant), Some(base)) = (
            columns.first().and_then(|c| single(c)),
            columns.get(1).and_then(|c| single(c)),
        ) else {
            bail!("mutants.tsv line {n}: a column does not give exactly 1 character: {line:?}");
        };
        if columns.len() > 2 {
            bail!("mutants.tsv line {n}: more than 2 columns: {line:?}");
        }
        if mutant == base {
            bail!("mutants.tsv line {n}: the mutant and base are the same character, {base}");
        }
        if !seen.insert(mutant) {
            bail!("mutants.tsv line {n}: the mutant {mutant} occurs 2 times");
        }
        rows.push((mutant, base));
    }
    Ok(rows)
}

/// Normalized character of a column that holds exactly 1 character.
fn single(column: &str) -> Option<char> {
    let mut chars = column.trim().chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => normalize_char(c),
        _ => None,
    }
}

/// Mutants that are not a KanjiVG component, in list order (step 6).
pub fn not_kanjivg_components(rows: &[(char, char)], components: &HashSet<char>) -> Vec<char> {
    let mut result = Vec::new();
    for &(mutant, _) in rows {
        if !components.contains(&mutant) && !result.contains(&mutant) {
            result.push(mutant);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_row() {
        assert_eq!(read("亻\t人\n").unwrap(), vec![('亻', '人')]);
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        assert!(read("# 亻\t人\n\n").unwrap().is_empty());
    }

    #[test]
    fn column_with_two_characters_fails() {
        let error = read("# comment\n亻亻\t人\n").unwrap_err().to_string();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn mutant_equal_to_its_base_fails() {
        // ⼈ (U+2F08, KANGXI RADICAL MAN) normalizes to 人.
        let error = read("\u{2F08}\t人\n").unwrap_err().to_string();
        assert!(error.contains("same character"), "{error}");
    }

    #[test]
    fn duplicate_row_fails() {
        let error = read("亻\t人\n亻\t人\n").unwrap_err().to_string();
        assert!(error.contains("2 times"), "{error}");
    }

    #[test]
    fn mutant_with_two_bases_fails() {
        let error = read("氵\t水\n氵\t氷\n").unwrap_err().to_string();
        assert!(
            error.contains("line 2") && error.contains("2 times"),
            "{error}"
        );
    }

    #[test]
    fn mound_and_city_are_two_mutants() {
        assert_eq!(
            read("\u{2ED6}\t阜\n\u{2ECF}\t邑\n").unwrap(),
            vec![('\u{2ED6}', '阜'), ('\u{2ECF}', '邑')]
        );
    }

    #[test]
    fn mutant_that_is_not_a_kanjivg_component_is_reported() {
        let rows = read("亻\t人\n罒\t网\n").unwrap();
        assert_eq!(rows.len(), 2);
        let components = HashSet::from(['亻']);
        assert_eq!(not_kanjivg_components(&rows, &components), vec!['罒']);
    }

    #[test]
    fn list_of_this_repository_is_valid() {
        let rows = read(MUTANTS).unwrap();
        assert_eq!(rows.len(), 23);
        assert!(rows.contains(&('艹', '艸')) && rows.contains(&('⻞', '食')));
    }
}
