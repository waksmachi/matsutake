//! Bridge from Dart to `jpdag::dictionary`: 1 function for each call of the API, and structs that
//! mirror its types. `flutter_rust_bridge` reads this module. It holds no logic.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::Result;
use flutter_rust_bridge::frb;

/// A `content.db`, open read-only. Dart holds a handle, not the data. The `Mutex` is there
/// because 1 SQLite connection serves 1 thread at a time.
#[frb(opaque)]
pub struct Dictionary(Mutex<jpdag::Dictionary>);

impl Dictionary {
    fn lock(&self) -> MutexGuard<'_, jpdag::Dictionary> {
        // A panic in a query leaves the connection usable, so a poisoned lock is not an error.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub struct SearchResults {
    pub characters: Vec<CharacterLink>,
    pub words: Vec<WordSummary>,
}

pub struct WordSummary {
    pub id: u32,
    pub headword: String,
    pub reading: Option<String>,
    pub matched_form: Option<String>,
    pub gloss: String,
    pub kanji: Option<String>,
    pub furigana: Vec<FuriganaPart>,
}

pub struct Word {
    pub id: u32,
    pub headword: String,
    pub written_forms: Vec<WrittenForm>,
    pub readings: Vec<Reading>,
    pub senses: Vec<Sense>,
}

pub struct WrittenForm {
    pub text: String,
    pub kanji: Vec<CharacterLink>,
    pub tags: Vec<Tag>,
    pub readings: Vec<FormReading>,
}

pub struct FormReading {
    pub text: String,
    pub furigana: Vec<FuriganaPart>,
}

pub struct FuriganaPart {
    pub text: String,
    pub kana: Option<String>,
}

pub struct Reading {
    pub text: String,
    pub no_kanji: bool,
    pub restricted_to: Vec<String>,
    pub tags: Vec<Tag>,
    pub morae: Vec<String>,
    pub accents: Vec<Accent>,
}

pub struct Accent {
    pub downstep: u32,
    pub parts_of_speech: Vec<Tag>,
}

pub struct Sense {
    pub glosses: Vec<Gloss>,
    pub note: Option<String>,
    pub tags: Vec<Tag>,
    pub written_forms: Vec<String>,
    pub readings: Vec<String>,
}

pub struct Gloss {
    pub text: String,
    pub gloss_type: Option<String>,
}

pub struct Tag {
    pub category: String,
    pub name: String,
    pub description: Option<String>,
}

pub struct Kanji {
    pub character: String,
    pub readings: Vec<CharacterReading>,
    pub meanings: Vec<String>,
    pub components: Vec<CharacterLink>,
    pub used_in: Vec<CharacterLink>,
    pub forms: Vec<Form>,
    pub entries: Vec<Word>,
    pub words: KanjiWords,
    pub word_count: u32,
}

pub struct CharacterReading {
    pub kind: ReadingKind,
    pub text: String,
    pub word_count: u32,
}

pub enum ReadingKind {
    On,
    Kun,
}

pub struct KanjiWords {
    pub starting: Vec<WordSummary>,
    pub ending: Vec<WordSummary>,
    pub inside: Vec<WordSummary>,
}

/// A link to a character. Each character is a `String` of 1 character.
pub struct CharacterLink {
    pub character: String,
    pub base: Option<String>,
}

pub struct Form {
    pub mutant: String,
    pub used_in: Vec<CharacterLink>,
}

pub fn open_dictionary(path: String) -> Result<Dictionary> {
    let dictionary = jpdag::Dictionary::open(Path::new(&path))?;
    Ok(Dictionary(Mutex::new(dictionary)))
}

pub fn search(
    dictionary: &Dictionary,
    query: String,
    offset: u32,
    limit: u32,
) -> Result<SearchResults> {
    let results = dictionary
        .lock()
        .search(&query, offset as usize, limit as usize)?;
    Ok(results.into())
}

pub fn word(dictionary: &Dictionary, id: u32) -> Result<Option<Word>> {
    Ok(dictionary.lock().word(id)?.map(Word::from))
}

/// Gives the page of the first character of `character`.
pub fn character(
    dictionary: &Dictionary,
    character: String,
    word_limit: u32,
) -> Result<Option<Kanji>> {
    let Some(character) = character.chars().next() else {
        return Ok(None);
    };
    let kanji = dictionary
        .lock()
        .character(character, word_limit as usize)?;
    Ok(kanji.map(Kanji::from))
}

fn convert<T, U: From<T>>(items: Vec<T>) -> Vec<U> {
    items.into_iter().map(U::from).collect()
}

impl From<jpdag::dictionary::SearchResults> for SearchResults {
    fn from(results: jpdag::dictionary::SearchResults) -> Self {
        SearchResults {
            characters: convert(results.characters),
            words: convert(results.words),
        }
    }
}

impl From<jpdag::dictionary::WordSummary> for WordSummary {
    fn from(summary: jpdag::dictionary::WordSummary) -> Self {
        WordSummary {
            id: summary.id,
            headword: summary.headword,
            reading: summary.reading,
            matched_form: summary.matched_form,
            gloss: summary.gloss,
            kanji: summary.kanji.map(String::from),
            furigana: convert(summary.furigana),
        }
    }
}

impl From<jpdag::dictionary::Word> for Word {
    fn from(word: jpdag::dictionary::Word) -> Self {
        Word {
            id: word.id,
            headword: word.headword,
            written_forms: convert(word.written_forms),
            readings: convert(word.readings),
            senses: convert(word.senses),
        }
    }
}

impl From<jpdag::dictionary::WrittenForm> for WrittenForm {
    fn from(form: jpdag::dictionary::WrittenForm) -> Self {
        WrittenForm {
            text: form.text,
            kanji: convert(form.kanji),
            tags: convert(form.tags),
            readings: convert(form.readings),
        }
    }
}

impl From<jpdag::dictionary::FormReading> for FormReading {
    fn from(reading: jpdag::dictionary::FormReading) -> Self {
        FormReading {
            text: reading.text,
            furigana: convert(reading.furigana),
        }
    }
}

impl From<jpdag::dictionary::FuriganaPart> for FuriganaPart {
    fn from(part: jpdag::dictionary::FuriganaPart) -> Self {
        FuriganaPart {
            text: part.text,
            kana: part.kana,
        }
    }
}

impl From<jpdag::dictionary::Reading> for Reading {
    fn from(reading: jpdag::dictionary::Reading) -> Self {
        Reading {
            text: reading.text,
            no_kanji: reading.no_kanji,
            restricted_to: reading.restricted_to,
            tags: convert(reading.tags),
            morae: reading.morae,
            accents: convert(reading.accents),
        }
    }
}

impl From<jpdag::dictionary::Accent> for Accent {
    fn from(accent: jpdag::dictionary::Accent) -> Self {
        Accent {
            downstep: accent.downstep,
            parts_of_speech: convert(accent.parts_of_speech),
        }
    }
}

impl From<jpdag::dictionary::Sense> for Sense {
    fn from(sense: jpdag::dictionary::Sense) -> Self {
        Sense {
            glosses: convert(sense.glosses),
            note: sense.note,
            tags: convert(sense.tags),
            written_forms: sense.written_forms,
            readings: sense.readings,
        }
    }
}

impl From<jpdag::dictionary::Gloss> for Gloss {
    fn from(gloss: jpdag::dictionary::Gloss) -> Self {
        Gloss {
            text: gloss.text,
            gloss_type: gloss.gloss_type,
        }
    }
}

impl From<jpdag::dictionary::Tag> for Tag {
    fn from(tag: jpdag::dictionary::Tag) -> Self {
        Tag {
            category: tag.category,
            name: tag.name,
            description: tag.description,
        }
    }
}

impl From<jpdag::dictionary::Kanji> for Kanji {
    fn from(kanji: jpdag::dictionary::Kanji) -> Self {
        Kanji {
            character: kanji.character.into(),
            readings: convert(kanji.readings),
            meanings: kanji.meanings,
            components: convert(kanji.components),
            used_in: convert(kanji.used_in),
            forms: convert(kanji.forms),
            entries: convert(kanji.entries),
            words: kanji.words.into(),
            word_count: kanji.word_count,
        }
    }
}

impl From<jpdag::dictionary::CharacterReading> for CharacterReading {
    fn from(reading: jpdag::dictionary::CharacterReading) -> Self {
        CharacterReading {
            kind: match reading.kind {
                jpdag::dictionary::ReadingKind::On => ReadingKind::On,
                jpdag::dictionary::ReadingKind::Kun => ReadingKind::Kun,
            },
            text: reading.text,
            word_count: reading.word_count,
        }
    }
}

impl From<jpdag::dictionary::KanjiWords> for KanjiWords {
    fn from(words: jpdag::dictionary::KanjiWords) -> Self {
        KanjiWords {
            starting: convert(words.starting),
            ending: convert(words.ending),
            inside: convert(words.inside),
        }
    }
}

impl From<jpdag::dictionary::CharacterLink> for CharacterLink {
    fn from(link: jpdag::dictionary::CharacterLink) -> Self {
        CharacterLink {
            character: link.character.into(),
            base: link.base.map(String::from),
        }
    }
}

impl From<jpdag::dictionary::Form> for Form {
    fn from(form: jpdag::dictionary::Form) -> Self {
        Form {
            mutant: form.mutant.into(),
            used_in: convert(form.used_in),
        }
    }
}
