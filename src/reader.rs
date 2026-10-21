use std::io::{BufRead, Read};

use memchr::memchr;

use crate::{Error, Format, Record, RT};

/// Reads records in sequence from a stream `R`, in a given format.
///
/// Each call to [`Reader::read`] extracts and parses the next record,
/// without loading the whole stream into memory. For `format: Text`,
/// records must be separated by a blank line (which is what
/// [`crate::Writer`] produces for this format).
///
/// A `Reader` is also an [`Iterator`] over `Result<Record, _>`: a
/// malformed record yields an `Err` and iteration goes on with the next
/// one, while an I/O error ends the iteration after being yielded.
///
/// ```no_run
/// use marcr::{Format, Reader};
/// use std::io::BufReader;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let input = BufReader::new(std::fs::File::open("notices.mrc")?);
/// let mut reader = Reader::new(Format::Iso2709, input);
/// for result in &mut reader {
///     match result {
///         Ok(record) => println!("{record}"),
///         Err(e) => eprintln!("skipped record: {e}"),
///     }
/// }
/// eprintln!("{} records read", reader.count);
/// # Ok(())
/// # }
/// ```
pub struct Reader<R> {
    pub format: Format,
    pub reader: R,
    pub buffer: Vec<u8>,   // Reusable internal buffer
    /// Number of records extracted from the stream so far, malformed ones
    /// included: after an error, it is the number of the faulty record.
    pub count: usize,
    /// Set by the iterator after an I/O error, to stop iterating.
    io_failed: bool,
    /// Bytes consumed from the stream so far, to locate malformed records.
    position: u64,
}

impl<R: Read + BufRead> Reader<R> {
    /// Creates a reader for `format` on top of the buffered stream `reader`.
    pub fn new(format: Format, reader: R) -> Self {
        let buffer: Vec<u8> = Vec::new();
        let count = 0;
        Self { format, reader, buffer, count, io_failed: false, position: 0 }
    }

    /// Reads and parses the next record from the stream.
    ///
    /// Returns `Ok(None)` at end of stream, [`Error::Io`] on a read error,
    /// [`Error::Malformed`] on a malformed record, with its number and the
    /// byte offset where reading of it started. A malformed record is
    /// consumed from the stream before its error is returned, so reading
    /// can go on with the next record by calling `read` again.
    pub fn read(&mut self) -> Result<Option<Record>, Error> {
        self.read_as(self.format)
    }

    /// Reads the next ISO 2709 record, whatever `self.format`: looks for
    /// the RT byte (`0x1d`) that terminates it in the stream.
    pub fn read_iso2709(&mut self) -> Result<Option<Record>, Error> {
        self.read_as(Format::Iso2709)
    }

    /// Reads the next MARCXML record, whatever `self.format`: looks for
    /// the next `<record>` element (with or without a namespace prefix or
    /// attributes) then captures up to and including its closing tag.
    pub fn read_marcxml(&mut self) -> Result<Option<Record>, Error> {
        self.read_as(Format::Marcxml)
    }

    /// Reads the next text record, whatever `self.format`: records are
    /// separated by a blank line (two consecutive `\n` bytes), the last one
    /// may end at end of stream.
    pub fn read_text(&mut self) -> Result<Option<Record>, Error> {
        self.read_as(Format::Text)
    }

    /// Reads the next MARC-in-JSON record, whatever `self.format`: the
    /// next top-level JSON object, in an array or not.
    pub fn read_json(&mut self) -> Result<Option<Record>, Error> {
        self.read_as(Format::Json)
    }

    /// Reads the next MARCMaker (`.mrk`) record, whatever `self.format`:
    /// records are separated by a blank line, as for [`Reader::read_text`].
    pub fn read_mrk(&mut self) -> Result<Option<Record>, Error> {
        self.read_as(Format::Mrk)
    }

    /// Extracts and parses the next record in `format`.
    fn read_as(&mut self, format: Format) -> Result<Option<Record>, Error> {
        match self.extract(format, |octets| format.deserialize(octets))? {
            Some((result, number, start)) => result.map(Some).map_err(|e| e.at(number, start)),
            None => Ok(None),
        }
    }

    /// Extracts the bytes of the next record in `format` and passes them to
    /// `f`, without parsing them. Returns `f`'s result with the record
    /// number and the byte offset where reading of the record started, or
    /// `None` at end of stream. The record is consumed and counted.
    pub(crate) fn extract<T>(
        &mut self,
        format: Format,
        mut f: impl FnMut(&[u8]) -> T,
    ) -> std::io::Result<Option<(T, usize, u64)>> {
        let start = self.position;
        let value = match format {
            Format::Iso2709 => self.extract_iso2709(&mut f)?,
            Format::Marcxml => self.extract_marcxml(&mut f)?,
            Format::Text | Format::Mrk => self.extract_text(&mut f)?,
            Format::Json    => self.extract_json(&mut f)?,
        };
        Ok(value.map(|value| {
            self.count += 1;
            (value, self.count, start)
        }))
    }

    /// ISO 2709 extraction: looks for the RT byte (`0x1d`) that
    /// terminates the record, accumulating into an internal buffer if it
    /// spans several `BufReader` reads.
    fn extract_iso2709<T>(&mut self, f: &mut impl FnMut(&[u8]) -> T) -> std::io::Result<Option<T>> {
        // Fallback buffer, only for records larger than the BufReader's capacity
        self.buffer.clear();
        let mut found = false;
        // `f`'s result is kept until the record bytes are consumed, so
        // that a malformed record doesn't block the stream.
        let mut option_result = None;
        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // End of reader without finding RT. Return None
                break;
            }
            let consumed = {
                if let Some(pos) = memchr(RT, available) {
                    if self.buffer.is_empty() {
                        // Found in reader buffer. No need to use an internal buffer, ie zero-copy
                        let octets = &available[..=pos];
                        option_result = Some(f(octets));
                    } else {
                        // Found a record which was extended on several reader buffer
                        self.buffer.extend_from_slice(&available[..=pos]);
                        option_result = Some(f(&self.buffer));
                    }
                    found = true;
                    pos + 1
                } else {
                    self.buffer.extend_from_slice(available);
                    available.len()
                }
            };
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        Ok(option_result)
    }

    /// MARCXML extraction: looks for the next `<record>` element, with or
    /// without a namespace prefix (`<marc:record>`) or attributes (e.g.
    /// `xmlns="..."`), then captures up to and including the matching
    /// closing tag (`</record>`, `</marc:record>`).
    fn extract_marcxml<T>(&mut self, f: &mut impl FnMut(&[u8]) -> T) -> std::io::Result<Option<T>> {
        self.buffer.clear();
        let mut found = false;
        // Kept until the record bytes are consumed (see read_iso2709).
        let mut option_result = None;

        // Locating <record>: after each '<', collects the tag name up to a
        // tag-name boundary (space/tab/EOL, '>' or '/'), and keeps it if
        // it is "record" or "prefix:record" — not "records", "recordData"...
        enum State { Searching, Name, OpenTag, Content }
        let mut state = State::Searching;
        let mut end_tag: Vec<u8> = Vec::new(); // "</record>" or "</prefix:record>"

        while !found {
            // 1. Direct access to the BufReader's memory buffer
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                break; // Clean end of file
            }

            let mut consumed = 0;
            while consumed < available.len() {
                let b = available[consumed];
                match state {
                    State::Content => {
                        // Jumps to the next '>', which may end the closing tag
                        let rest = &available[consumed..];
                        let len = memchr(b'>', rest).map_or(rest.len(), |pos| pos + 1);
                        self.buffer.extend_from_slice(&rest[..len]);
                        consumed += len;
                        if self.buffer.ends_with(&end_tag) {
                            found = true;
                            option_result = Some(f(&self.buffer));
                            break;
                        }
                        continue;
                    },
                    State::Searching => {
                        if b == b'<' {
                            self.buffer.push(b);
                            state = State::Name;
                        }
                    },
                    State::Name => {
                        if b.is_ascii_whitespace() || b == b'>' || b == b'/' {
                            let name = &self.buffer[1..];
                            if name == b"record" || name.ends_with(b":record") {
                                end_tag.clear();
                                end_tag.extend_from_slice(b"</");
                                end_tag.extend_from_slice(name);
                                end_tag.push(b'>');
                                self.buffer.push(b);
                                state = if b == b'>' { State::Content } else { State::OpenTag };
                            } else {
                                self.buffer.clear();
                                state = State::Searching;
                            }
                        } else if b == b'<' {
                            self.buffer.clear();
                            self.buffer.push(b);
                        } else if self.buffer.len() > 64 {
                            // Too long for a tag name (comment, CDATA...): give up
                            self.buffer.clear();
                            state = State::Searching;
                        } else {
                            self.buffer.push(b);
                        }
                    },
                    State::OpenTag => {
                        self.buffer.push(b);
                        if b == b'>' {
                            state = State::Content;
                        }
                    },
                }
                consumed += 1;
            }

            // 2. Tell the BufReader that `consumed` bytes have been processed
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        Ok(option_result)
    }

    /// Text and MARCMaker extraction: accumulates bytes until a blank line
    /// (`\n\n`, or `\n\r\n` with CRLF line endings), which separates two
    /// records — the separator produced by [`crate::Writer`] for these
    /// formats. Extra blank lines between records are skipped. The last
    /// record in the stream, not followed by a blank line, is accepted at
    /// end of stream (EOF) if there is accumulated content left.
    fn extract_text<T>(&mut self, f: &mut impl FnMut(&[u8]) -> T) -> std::io::Result<Option<T>> {
        self.buffer.clear();
        let mut found = false;
        // Kept until the record bytes are consumed (see read_iso2709).
        let mut option_result = None;

        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // End of stream: the last record isn't followed by a
                // blank line, decode what's left if there is any.
                if !self.buffer.is_empty() {
                    option_result = Some(f(&self.buffer));
                }
                break;
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                if self.buffer.is_empty() && (b == b'\n' || b == b'\r') {
                    continue; // Extra blank line before a record
                }
                if b == b'\n' && (self.buffer.ends_with(b"\n") || self.buffer.ends_with(b"\n\r")) {
                    // Separating blank line, not included in the record.
                    if self.buffer.ends_with(b"\r") {
                        self.buffer.pop();
                    }
                    found = true;
                    option_result = Some(f(&self.buffer));
                    break;
                }
                self.buffer.push(b);
            }
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        Ok(option_result)
    }

    /// MARC-in-JSON extraction: captures the next top-level `{...}`
    /// object, counting braces outside JSON strings. Bytes between
    /// objects (`[`, `,`, `]`, whitespace) are skipped, so that a JSON
    /// array of records and concatenated objects (JSON Lines) are both
    /// accepted. An object unterminated at end of stream is passed to `f`
    /// as is, to be reported as malformed.
    fn extract_json<T>(&mut self, f: &mut impl FnMut(&[u8]) -> T) -> std::io::Result<Option<T>> {
        self.buffer.clear();
        let mut found = false;
        // Kept until the record bytes are consumed (see read_iso2709).
        let mut option_result = None;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;

        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                if !self.buffer.is_empty() {
                    option_result = Some(f(&self.buffer));
                }
                break;
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                if depth == 0 {
                    if b != b'{' {
                        continue; // Between objects
                    }
                } else if in_string {
                    if escaped {
                        escaped = false;
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'"' {
                        in_string = false;
                    }
                    self.buffer.push(b);
                    continue;
                }
                self.buffer.push(b);
                match b {
                    b'"' => in_string = true,
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            found = true;
                            option_result = Some(f(&self.buffer));
                            break;
                        }
                    },
                    _ => (),
                }
            }
            self.reader.consume(consumed);
            self.position += consumed as u64;
        }
        Ok(option_result)
    }
}

impl<R: Read + BufRead> Iterator for Reader<R> {
    type Item = Result<Record, Error>;

    /// Calls [`Reader::read`]. After an I/O error, which is not tied to a
    /// record and would likely repeat, returns `None`.
    fn next(&mut self) -> Option<Self::Item> {
        if self.io_failed {
            return None;
        }
        match self.read() {
            Ok(record) => record.map(Ok),
            Err(e) => {
                self.io_failed = e.is_io();
                Some(Err(e))
            }
        }
    }
}
