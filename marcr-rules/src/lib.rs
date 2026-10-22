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

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use marcr::{Field, Record};

mod engine;
mod syntax;
#[cfg(test)]
mod tests;

/// The rules files provided with the crate: (name, content).
pub const BUILTIN: &[(&str, &str)] = &[
    ("unimarc-to-marc21", include_str!("../rules/unimarc-to-marc21.rules")),
    ("marc21-to-unimarc", include_str!("../rules/marc21-to-unimarc.rules")),
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
        engine::apply(&self.parsed, record, None)
    }

    /// Converts `record` like [`Rules::apply`], and reports what the rules
    /// leave out of it.
    pub fn apply_with_report(&self, record: &Record) -> (Record, Report) {
        let report = RefCell::new(Report::default());
        let converted = engine::apply(&self.parsed, record, Some(&report));
        (converted, report.into_inner())
    }
}

/// What [`Rules::apply_with_report`] leaves out of a record: neither
/// converted by a rule nor read by an expression.
#[derive(Debug, Default)]
pub struct Report {
    /// Tags of the fields that no rule converts (`*` rules aside).
    pub unconverted_fields: Vec<u16>,
    /// Tag and code of the subfields that no body line selects, in fields
    /// converted to data fields.
    pub unconverted_subfields: Vec<(u16, char)>,
    /// Table name and key of the lookups of a key missing from a table
    /// without a `*` entry.
    pub table_misses: Vec<(String, String)>,
    /// Indexes of the fields no rule converts, and of the subfields no
    /// body line selects (field, subfield).
    left_fields: Vec<usize>,
    left_subfields: Vec<(usize, usize)>,
    /// Fields, or subfields of a code, read by an expression: not left out.
    read: HashSet<(usize, Option<char>)>,
}

impl Report {
    /// Fills the public lists from what was left out and not read.
    fn finish(&mut self, record: &Record) {
        let read = |i: usize, code: Option<char>| self.read.contains(&(i, None)) || self.read.contains(&(i, code));
        let fields = self.left_fields.iter()
            .filter(|i| !self.read.iter().any(|(j, _)| j == *i))
            .map(|i| *record.fields[*i].tag());
        let subfields = self.left_subfields.iter().filter_map(|(i, k)| match &record.fields[*i] {
            Field::Standard(tag, _, subfields) if !read(*i, Some(subfields[*k].0)) => Some((*tag, subfields[*k].0)),
            _ => None,
        });
        (self.unconverted_fields, self.unconverted_subfields) = (fields.collect(), subfields.collect());
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
