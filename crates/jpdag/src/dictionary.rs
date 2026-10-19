//! Dictionary: queries over the JMdict-derived tables of `content.db`, as
//! `Docs/jpdag-dictionary-design.md` gives them.

use std::path::Path;

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags, Params};
use serde::de::DeserializeOwned;

use crate::kana::length_without_kana;
use crate::schema::{COMMON_RANK, SCHEMA_VERSION};

mod character;
mod search;
#[cfg(test)]
mod tests;
mod word;

pub use character::{CharacterLink, CharacterReading, Form, Kanji, KanjiWords, ReadingKind};
pub use search::{SearchResults, WordSummary};
pub use word::{Accent, FormReading, FuriganaPart, Gloss, Reading, Sense, Tag, Word, WrittenForm};

/// Number of prepared statements that the connection keeps: more than the queries of this module.
const STATEMENT_CACHE: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cannot read a row: {0}")]
    Row(#[from] serde_rusqlite::Error),
    #[error("content.db has schema version {found}, and this build reads version {expected}")]
    SchemaVersion { found: i32, expected: i32 },
}

/// A `content.db`, open read-only.
pub struct Dictionary {
    conn: Connection,
}

impl Dictionary {
    /// Opens the `content.db` at `path`. A file with another schema version gives
    /// [`Error::SchemaVersion`].
    pub fn open(path: &Path) -> Result<Dictionary, Error> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)?;
        let found: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if found != SCHEMA_VERSION {
            return Err(Error::SchemaVersion {
                found,
                expected: SCHEMA_VERSION,
            });
        }
        conn.set_prepared_statement_cache_capacity(STATEMENT_CACHE);
        let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
        // The order of the words of a kanji counts the characters that are not kana.
        conn.create_scalar_function("length_without_kana", 1, flags, |context| {
            let text: String = context.get(0)?;
            Ok(length_without_kana(&text) as i64)
        })?;
        // The orders by frequency use the band of a rank.
        conn.create_scalar_function("rank_band", 1, flags, |context| {
            let rank: Option<i64> = context.get(0)?;
            Ok(rank_band(rank))
        })?;
        Ok(Dictionary { conn })
    }

    /// Runs a query, and maps each result row to a `T` by column name.
    fn rows<T: DeserializeOwned>(&self, sql: &str, params: impl Params) -> Result<Vec<T>, Error> {
        let mut statement = self.conn.prepare_cached(sql)?;
        let rows = serde_rusqlite::from_rows::<T>(statement.query(params)?);
        let rows = rows.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Runs a query that gives 1 row at most.
    fn row<T: DeserializeOwned>(&self, sql: &str, params: impl Params) -> Result<Option<T>, Error> {
        Ok(self.rows(sql, params)?.into_iter().next())
    }
}

/// Gives the place of a frequency rank in an order of words: the rank itself up to
/// [`COMMON_RANK`], 1 place for each rarer rank, and 1 place after it for no rank.
fn rank_band(rank: Option<i64>) -> i64 {
    let common = i64::from(COMMON_RANK);
    match rank {
        Some(rank) if rank <= common => rank,
        Some(_) => common + 1,
        None => common + 2,
    }
}

/// The best frequency rank of the word `?1`: of the word in 1 of its written forms, or in kana.
/// A rank in kana counts for a word that JMdict writes in kana (a word with no written form, or a
/// word whose first sense has the tag `uk`), and for a reading that no other word has: きれい of
/// 綺麗 is rank 675 in kana, and 7364 as 綺麗. Jiten counts a reading in kana for the words of
/// other words with the same sound, so the kana rank of a shared reading does not count: なれる
/// (402) also counts なれる of なる, so 慣れる keeps its rank as 慣れる (948). Each order by
/// frequency uses this rank, with `?1` replaced by the column of the word id.
///
/// Its aliases are its own, so that the column can be of a table that the query calls `f` or `r`.
const WORD_RANK: &str = "(
  SELECT min(rank) FROM (
    SELECT rank_ff.rank FROM written_form rank_f
    JOIN written_form_frequency rank_ff ON rank_ff.written_form_id = rank_f.id
    WHERE rank_f.word_id = ?1
    UNION ALL
    SELECT rank_rf.rank FROM reading rank_r
    JOIN reading_frequency rank_rf ON rank_rf.reading_id = rank_r.id
    WHERE rank_r.word_id = ?1 AND (
      NOT EXISTS (SELECT 1 FROM written_form rank_g WHERE rank_g.word_id = ?1)
      OR NOT EXISTS (
        SELECT 1 FROM reading rank_o
        WHERE rank_o.text = rank_r.text AND rank_o.word_id != rank_r.word_id)
      OR EXISTS (
        SELECT 1 FROM sense rank_s
        JOIN sense_tag rank_st ON rank_st.sense_id = rank_s.id
        JOIN tag rank_t ON rank_t.id = rank_st.tag_id
        WHERE rank_s.word_id = ?1 AND rank_s.position = 1 AND rank_t.name = 'uk'))))";

/// Gives [`WORD_RANK`] for the word id in `column`.
fn word_rank(column: &str) -> String {
    WORD_RANK.replace("?1", column)
}

/// Gives `count` as the value of a `LIMIT` or an `OFFSET`.
fn sql_count(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}
