# Mozc romaji table

`romanji-hiragana.tsv` is the romaji table of [Mozc](https://github.com/google/mozc), Google's
open-source input method. `jpdag::kana` uses it to convert romaji to hiragana.

- Source: `src/data/preedit/romanji-hiragana.tsv`, at commit
  `60af02ff797275f2ba1b7fddccdec916798d112e` (2025-04-01)
- Licence: BSD 3-Clause, in `LICENSE`
- The file is an exact copy. Each line has 2 or 3 columns, separated by a tab: the letters, the
  text that they give, and the letters that stay for the next row.
