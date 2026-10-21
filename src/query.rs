//! Record selection (`--where`) and extraction (`--fields`) for the CLI.

use std::borrow::Cow;
use std::fmt;

use marcr::{Field, Record};
use regex::Regex;

/// A field or subfield designation: `200` or `200a`.
#[derive(Clone, Debug)]
pub struct Path {
    tag: u16,
    code: Option<char>,
}

impl Path {
    /// Parses `TAG[c]` at the start of `s`, returning the rest.
    fn parse_prefix(s: &str) -> Result<(Path, &str), String> {
        let tag = s.get(..3)
            .filter(|tag| tag.bytes().all(|b| b.is_ascii_digit()))
            .ok_or_else(|| format!("{s:?}: expected a 3-digit tag"))?;
        let tag: u16 = tag.parse().unwrap();
        let mut rest = &s[3..];
        let mut chars = rest.chars();
        let code = chars.next().filter(|c| !matches!(c, '=' | '~'));
        if code.is_some() {
            rest = chars.as_str();
        }
        if tag < 10 && code.is_some() {
            return Err(format!("{s:?}: control field {tag:03} has no subfields"));
        }
        Ok((Path { tag, code }, rest))
    }

    /// Parses `TAG` or `TAGc`.
    pub fn parse(s: &str) -> Result<Path, String> {
        match Path::parse_prefix(s)? {
            (path, "") => Ok(path),
            _ => Err(format!("{s:?}: expected a tag, optionally followed by a subfield code")),
        }
    }

    /// The values at this path, one per occurrence: control field values,
    /// values of the `code` subfields, or, without a code, the subfields
    /// of each standard field joined by a space.
    fn values<'a>(&self, record: &'a Record) -> Vec<Cow<'a, str>> {
        let mut values = Vec::new();
        for field in record.fields_by_tag(self.tag) {
            match (field, self.code) {
                (Field::Control(_, value), _) => values.push(Cow::Borrowed(value.as_str())),
                (field, Some(code)) => values.extend(field.subfields(code).map(Cow::Borrowed)),
                (Field::Standard(_, _, subfields), None) => {
                    let text: Vec<&str> = subfields.iter().map(|subfield| subfield.1.as_str()).collect();
                    values.push(Cow::Owned(text.join(" ")));
                }
            }
        }
        values
    }
}

/// `200` or `200a`, as given on the command line.
impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:03}", self.tag)?;
        match self.code {
            Some(code) => write!(f, "{code}"),
            None => Ok(()),
        }
    }
}

/// A `--where` condition: `[!]TAG[c]`, `[!]TAG[c]=VALUE` or
/// `[!]TAG[c]~REGEX`.
#[derive(Clone, Debug)]
pub struct Condition {
    negated: bool,
    path: Path,
    test: Test,
}

#[derive(Clone, Debug)]
enum Test {
    Exists,
    Equals(String),
    Matches(Regex),
}

impl Condition {
    pub fn parse(s: &str) -> Result<Condition, String> {
        let (negated, rest) = match s.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (path, rest) = Path::parse_prefix(rest)?;
        let test = if rest.is_empty() {
            Test::Exists
        } else if let Some(value) = rest.strip_prefix('=') {
            Test::Equals(value.to_string())
        } else if let Some(regex) = rest.strip_prefix('~') {
            Test::Matches(Regex::new(regex).map_err(|e| format!("{s:?}: {e}"))?)
        } else {
            return Err(format!("{s:?}: expected = or ~ after {path}"));
        };
        Ok(Condition { negated, path, test })
    }

    /// Whether `record` has a value at the path (that equals or matches,
    /// with `=` or `~`), the reverse if negated.
    pub fn matches(&self, record: &Record) -> bool {
        let values = self.path.values(record);
        let found = match &self.test {
            Test::Exists => !values.is_empty(),
            Test::Equals(expected) => values.iter().any(|value| value == expected),
            Test::Matches(regex) => values.iter().any(|value| regex.is_match(value)),
        };
        found != self.negated
    }
}

/// The TSV header line: the column names, tab-separated.
pub fn tsv_header(columns: &[Path]) -> String {
    columns.iter().map(Path::to_string).collect::<Vec<_>>().join("\t")
}

/// The TSV line of `record`: one column per path, repeated values joined
/// by `|`; tabs and line breaks in values are replaced by spaces.
pub fn tsv_row(columns: &[Path], record: &Record) -> String {
    columns.iter()
        .map(|path| path.values(record).join("|").replace(['\t', '\n', '\r'], " "))
        .collect::<Vec<_>>()
        .join("\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        let mut record = Record::default();
        record.insert(vec![
            vec!["001", "PPN1"],
            vec!["101", "  ", "a", "fre", "a", "lat"],
            vec!["200", "1 ", "a", "Les Misérables", "f", "Victor Hugo"],
        ]);
        record
    }

    fn matches(condition: &str) -> bool {
        Condition::parse(condition).unwrap().matches(&record())
    }

    #[test]
    fn conditions() {
        assert!(matches("200"));
        assert!(matches("200f"));
        assert!(!matches("200e"));
        assert!(matches("!856"));
        assert!(!matches("!200"));
        assert!(matches("001=PPN1"));
        assert!(matches("101a=lat")); // any repetition
        assert!(!matches("101a=fr"));
        assert!(matches("200a~^Les? "));
        assert!(matches("200~Misérables Victor")); // whole field
        assert!(!matches("!200f~Hugo"));
    }

    #[test]
    fn invalid_conditions() {
        for condition in ["20", "abc", "001a", "200ab", "200a~(", ""] {
            assert!(Condition::parse(condition).is_err(), "{condition}");
        }
    }

    #[test]
    fn tsv() {
        let columns: Vec<Path> = ["001", "101a", "200a", "700a"].iter().map(|s| Path::parse(s).unwrap()).collect();
        assert_eq!(tsv_header(&columns), "001\t101a\t200a\t700a");
        assert_eq!(tsv_row(&columns, &record()), "PPN1\tfre|lat\tLes Misérables\t");
        assert!(Path::parse("200a=x").is_err());
    }
}
