//! Dictionary: queries over the JMdict-derived tables of `content.db`, as
//! `Docs/jpdag-dictionary-design.md` gives them.

use std::path::Path;

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags, Params};
use serde::de::DeserializeOwned;

use crate::kana::length_without_kana;
use crate::schema::SCHEMA_VERSION;

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

/// Gives `count` as the value of a `LIMIT` or an `OFFSET`.
fn sql_count(count: usize) -> i64 {
    i64::try_from(count).unwrap_or(i64::MAX)
}
