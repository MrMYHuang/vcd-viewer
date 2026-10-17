use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

pub trait VcdByteSource: Send + Sync {
    fn total_len(&self) -> u64;
    fn read_chunk(&self, offset: u64, max_len: usize) -> io::Result<Vec<u8>>;
}

pub struct MemByteSource {
    data: Vec<u8>,
}

impl MemByteSource {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl VcdByteSource for MemByteSource {
    fn total_len(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_chunk(&self, offset: u64, max_len: usize) -> io::Result<Vec<u8>> {
        let start = (offset as usize).min(self.data.len());
        let end = start.saturating_add(max_len).min(self.data.len());
        Ok(self.data[start..end].to_vec())
    }
}

pub struct FileByteSource {
    file: Mutex<File>,
    len: u64,
}

impl FileByteSource {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self {
            file: Mutex::new(file),
            len,
        })
    }
}

impl VcdByteSource for FileByteSource {
    fn total_len(&self) -> u64 {
        self.len
    }

    fn read_chunk(&self, offset: u64, max_len: usize) -> io::Result<Vec<u8>> {
        if offset >= self.len || max_len == 0 {
            return Ok(Vec::new());
        }
        let to_read = ((self.len - offset) as usize).min(max_len);
        let mut guard = self
            .file
            .lock()
            .map_err(|_| io::Error::other("FileByteSource mutex poisoned"))?;
        guard.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; to_read];
        let mut read_total = 0;
        while read_total < to_read {
            let n = guard.read(&mut buf[read_total..])?;
            if n == 0 {
                break;
            }
            read_total += n;
        }
        buf.truncate(read_total);
        Ok(buf)
    }
}

pub struct LineChunkReader<'a> {
    source: &'a dyn VcdByteSource,
    chunk_size: usize,
    next_fetch_offset: u64,
    buf: Vec<u8>,
    buf_pos: usize,
    buf_base_offset: u64,
    eof: bool,
}

impl<'a> LineChunkReader<'a> {
    pub fn new(source: &'a dyn VcdByteSource, start_offset: u64, chunk_size: usize) -> Self {
        Self {
            source,
            chunk_size: chunk_size.max(1),
            next_fetch_offset: start_offset,
            buf: Vec::new(),
            buf_pos: 0,
            buf_base_offset: start_offset,
            eof: false,
        }
    }

    pub fn current_offset(&self) -> u64 {
        self.buf_base_offset + self.buf_pos as u64
    }

    fn refill(&mut self) -> io::Result<bool> {
        if self.eof {
            return Ok(false);
        }
        let chunk = self
            .source
            .read_chunk(self.next_fetch_offset, self.chunk_size)?;
        if chunk.is_empty() {
            self.eof = true;
            return Ok(false);
        }
        self.next_fetch_offset += chunk.len() as u64;
        if self.buf_pos > 0 {
            self.buf.drain(0..self.buf_pos);
            self.buf_base_offset += self.buf_pos as u64;
            self.buf_pos = 0;
        }
        self.buf.extend_from_slice(&chunk);
        Ok(true)
    }

    pub fn next_token(&mut self) -> io::Result<Option<(u64, Vec<u8>)>> {
        // 1. Skip leading ASCII whitespace
        loop {
            while self.buf_pos < self.buf.len() && self.buf[self.buf_pos].is_ascii_whitespace() {
                self.buf_pos += 1;
            }
            if self.buf_pos < self.buf.len() {
                break;
            }
            if !self.refill()? {
                return Ok(None);
            }
        }

        let token_start_offset = self.buf_base_offset + self.buf_pos as u64;
        let mut token = Vec::new();

        // 2. Accumulate non-whitespace bytes across chunk boundaries
        loop {
            let start = self.buf_pos;
            while self.buf_pos < self.buf.len() && !self.buf[self.buf_pos].is_ascii_whitespace() {
                self.buf_pos += 1;
            }
            token.extend_from_slice(&self.buf[start..self.buf_pos]);
            if self.buf_pos < self.buf.len() {
                // Hit whitespace delimiter
                break;
            }
            if !self.refill()? {
                // Hit EOF
                break;
            }
        }

        Ok(Some((token_start_offset, token)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_chunk_reader_across_tiny_chunks() {
        let raw = b"#12345678\nb10101100 !\n#99999999\n1\"\n".to_vec();
        let src = MemByteSource::new(raw);
        // Use a 5-byte chunk size so every token splits across chunk boundaries
        let mut reader = LineChunkReader::new(&src, 0, 5);
        let mut tokens = Vec::new();
        while let Some((off, tok)) = reader.next_token().unwrap() {
            tokens.push((off, String::from_utf8(tok).unwrap()));
        }
        assert_eq!(
            tokens,
            vec![
                (0, "#12345678".to_string()),
                (10, "b10101100".to_string()),
                (20, "!".to_string()),
                (22, "#99999999".to_string()),
                (32, "1\"".to_string()),
            ]
        );
    }
}
