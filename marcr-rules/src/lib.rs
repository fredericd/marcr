//! Rule language to convert MARC records between formats, starting with
//! UNIMARC to MARC21. A rules file is self-contained: header, tables and
//! rules. See the README for the language.
//!
//! ```
//! use marcr_rules::Rules;
//!
//! let rules = Rules::builtin("unimarc-to-marc21").unwrap();
//! # let unimarc = marcr::Record::default();
//! let marc21 = rules.apply(&unimarc);
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

use marcr::Record;

mod engine;
mod syntax;
#[cfg(test)]
mod tests;

/// The rules files provided with the crate: (name, content).
pub const BUILTIN: &[(&str, &str)] = &[
    ("unimarc-to-marc21", include_str!("../rules/unimarc-to-marc21.rules")),
];

/// A compiled set of conversion rules.
pub struct Rules {
    parsed: syntax::Parsed,
}

impl Rules {
    /// The rules file provided with the crate under `name` (see
    /// [`BUILTIN`]), compiled.
    pub fn builtin(name: &str) -> Option<Rules> {
        BUILTIN.iter().find(|(n, _)| *n == name).map(|(name, text)| {
            let path = PathBuf::from(format!("{name}.rules"));
            Rules::parse(text, &path).unwrap_or_else(|e| panic!("built-in rules: {e}"))
        })
    }

    /// The name given by the `rules "..."` header, if any.
    pub fn name(&self) -> Option<&str> {
        self.parsed.name.as_deref()
    }

    /// The `version` of the rules file, if any.
    pub fn version(&self) -> Option<&str> {
        self.parsed.version.as_deref()
    }

    /// Reads and compiles the rules file at `path`.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Rules, Error> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error { file: path.to_path_buf(), line: 0, message: e.to_string() })?;
        Rules::parse(&text, path)
    }

    /// Compiles `text`, read from `path` (for error messages).
    pub fn parse(text: &str, path: &Path) -> Result<Rules, Error> {
        Ok(Rules { parsed: syntax::Parser::new(text, path).file()? })
    }

    /// Converts `record`. Fields are sorted by tag; a field that no rule
    /// converts is dropped, unless a `*` rule takes it.
    pub fn apply(&self, record: &Record) -> Record {
        engine::apply(&self.parsed, record)
    }
}

/// An error in a rules file, with its location
/// (line 0 when the file could not be read).
#[derive(Debug)]
pub struct Error {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            0 => write!(f, "{}: {}", self.file.display(), self.message),
            line => write!(f, "{}:{line}: {}", self.file.display(), self.message),
        }
    }
}

impl std::error::Error for Error {}
