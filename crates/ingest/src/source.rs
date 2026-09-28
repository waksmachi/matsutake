//! Opens a source file. Decompresses gzip data, and counts lines for the error messages.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use flate2::bufread::MultiGzDecoder;

pub type Source = LineCounter<Box<dyn BufRead>>;

/// Opens the file at `path`. If the file starts with the gzip magic number, the result is the
/// decompressed data.
pub fn open_path(path: &Path) -> Result<Source> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    open(BufReader::with_capacity(1 << 16, file))
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Gives the data of `input`. If `input` starts with the gzip magic number, the result is the
/// decompressed data.
pub fn open<R: BufRead + 'static>(mut input: R) -> io::Result<Source> {
    let gzip = input.fill_buf()?.starts_with(&[0x1f, 0x8b]);
    let inner: Box<dyn BufRead> = if gzip {
        Box::new(BufReader::with_capacity(
            1 << 16,
            MultiGzDecoder::new(input),
        ))
    } else {
        Box::new(input)
    };
    Ok(LineCounter { inner, line: 1 })
}

/// A reader that counts the lines of the data that its user consumes.
pub struct LineCounter<R> {
    inner: R,
    line: u64,
}

impl<R> LineCounter<R> {
    /// The line of the next byte that the user will consume. The first line is 1.
    pub fn line(&self) -> u64 {
        self.line
    }
}

fn newlines(bytes: &[u8]) -> u64 {
    bytes.iter().filter(|&&b| b == b'\n').count() as u64
}

impl<R: BufRead> Read for LineCounter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.line += newlines(&buf[..n]);
        Ok(n)
    }
}

impl<R: BufRead> BufRead for LineCounter<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        // The buffer is not empty when amt > 0, so fill_buf returns it without a read.
        if amt > 0
            && let Ok(buf) = self.inner.fill_buf()
        {
            self.line += newlines(&buf[..amt.min(buf.len())]);
        }
        self.inner.consume(amt);
    }
}
