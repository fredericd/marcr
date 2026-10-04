use std::fmt;
use std::str;

use crate::DEFAULT_LEADER;

/// A subfield of a "standard" MARC field (tag ≥ 10): a one-character
/// code (`$a`, `$b`, ...) and its value.
#[derive(Debug)]
pub struct Subfield(pub char, pub String);

impl fmt::Display for Subfield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Subfield(letter, value) = &self;
        write!(f, "${} {}", *letter as char, value)
    }
}

/// A field of a MARC record: either a control field (tag < 10, a plain
/// text value), or a standard field (tag ≥ 10, two indicators and a list
/// of subfields).
#[derive(Debug)]
pub enum Field {
    /// Control field (tags 001-009): tag and value.
    Control(u16, String),
    /// Standard field (tags ≥ 010): tag, the two indicators, and the
    /// subfields.
    Standard(u16, [char; 2], Vec<Subfield>),
}

impl Field {
    /// The field's tag (001-999).
    pub fn tag(&self) -> &u16 {
        match self {
            Field::Control(tag, _) => tag,
            Field::Standard(tag, _, _) => tag,
        }
    }

    /// The value of the first subfield with the given `code`, or `None`
    /// if there is none. Always `None` for a control field.
    ///
    /// ```
    /// use marcr::Record;
    ///
    /// let mut record = Record::default();
    /// record.insert(vec![vec!["676", "  ", "a", "843", "v", "23"]]);
    ///
    /// let dewey = record.field(676).and_then(|field| field.subfield('a'));
    /// assert_eq!(dewey, Some("843"));
    /// ```
    pub fn subfield(&self, code: char) -> Option<&str> {
        match self {
            Field::Control(_, _) => None,
            Field::Standard(_, _, subfields) => subfields
                .iter()
                .find(|subfield| subfield.0 == code)
                .map(|subfield| subfield.1.as_str()),
        }
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::Control(tag, value) => write!(f, "{tag:03} {value}"),
            Field::Standard(tag, ind, subfs) => {
                let ind1 = ind[0];
                let ind2 = ind[1];
                let concatenated = subfs
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(f, "{tag:03} {ind1}{ind2} {concatenated}")
            },
        }
    }
}

/// A MARC bibliographic record: a 24-byte leader and a list of fields.
#[derive(Debug)]
pub struct Record {
    /// The 24 leader bytes (record length, status, document type,
    /// directory position/size, etc.).
    pub leader: [u8; 24],
    /// The record's fields, sorted by increasing tag when added via
    /// [`Record::add`] or [`Record::insert`].
    pub fields: Vec<Field>,
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let leader: &str = unsafe { str::from_utf8_unchecked(&self.leader) };
        let text = self.fields
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{leader}\n{text}")
    }
}

impl Default for Record {
    fn default() -> Self {
        let leader: [u8; 24] = DEFAULT_LEADER;
        let fields: Vec<Field> = Vec::new();
        Self {
            leader,
            fields,
        }
    }
}

impl Record {
    /// Inserts `field` while keeping tags in increasing order (insertion
    /// sort: fields with the same tag keep their relative order).
    pub fn add(&mut self, field: Field) {
        let tag = field.tag();
        let pos: usize = self.fields.iter()
            .position(|item| item.tag() > tag)
            .unwrap_or(self.fields.len());
        self.fields.insert(pos, field);
    }

    /// Builds a record with the default leader and the given fields,
    /// without sorting them (unlike [`Record::add`]).
    pub fn new(fields: Vec<Field>) -> Self {
        let leader: [u8; 24] = DEFAULT_LEADER;
        Self {
            leader,
            fields,
        }
    }

    /// Builds and adds fields from a compact notation: each `Vec<&str>`
    /// is `[tag, indicators_or_value, code1, value1, code2, value2, ...]`.
    ///
    /// - For a control field (tag ≤ 9): `vec!["001", "PPN1234"]`.
    /// - For a standard field: `vec!["200", "  ", "a", "Title", "b", "Subtitle"]`
    ///   where `"  "` are the two indicators.
    ///
    /// Entries that are too short or malformed are silently ignored.
    /// Fields are added sorted by tag via [`Record::add`].
    pub fn insert(&mut self, a_a_a: Vec<Vec<&str>>) {
        for a_a in a_a_a {
            let len = a_a.len();
            if len < 2 { continue; }
            let tag_str = a_a[0];
            if let Ok(tag) = tag_str.parse::<u16>() {
                if tag <= 9 {
                    if len == 2 {
                        let field = Field::Control(tag, String::from(a_a[1]));
                        self.add(field);
                    }
                }
                else if tag <= 999 {
                    if len < 4 { continue; }
                    let ind_str = a_a[1];
                    let mut iter = ind_str.chars();
                    let ind: [char; 2] = [
                        iter.next().expect("Missing first character"),
                        iter.next().expect("Missing second character"),
                    ];
                    let mut subfields: Vec<Subfield> = Vec::new();
                    for i in (2..len-1).step_by(2) {
                        let letter_str = a_a[i];
                        let value_str = a_a[i+1];
                        if letter_str.len() >= 1 && value_str.len() > 1 {
                            let mut iter = letter_str.chars();
                            let letter = iter.next().expect("Not a letter for subfield");
                            let value = String::from(value_str);
                            subfields.push(Subfield(letter, value));
                        }
                    }
                    let field = Field::Standard(tag, ind, subfields);
                    self.add(field);
                }
            }
        }
    }

    /// Returns the first field with the given `tag`, or `None` if the
    /// record has none. Use [`Record::fields_by_tag`] to get all of them.
    ///
    /// ```
    /// use marcr::{Field, Record};
    ///
    /// let mut record = Record::default();
    /// record.insert(vec![
    ///     vec!["001", "PPN1234"],
    ///     vec!["200", " 1", "a", "Mon titre", "e", "Complément"],
    /// ]);
    ///
    /// if let Some(Field::Standard(_, _, subfields)) = record.field(200) {
    ///     assert_eq!(subfields[0].1, "Mon titre");
    /// }
    /// assert!(record.field(999).is_none());
    /// ```
    pub fn field(&self, tag: u16) -> Option<&Field> {
        self.fields.iter().find(|field| *field.tag() == tag)
    }

    /// Returns an iterator over the fields with the given `tag`, in their
    /// existing order.
    pub fn fields_by_tag(&self, tag: u16) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter(move |field| *field.tag() == tag)
    }

    /// Removes all fields with the given `tag` and returns them, in their
    /// former relative order. Fields with other tags keep their relative
    /// order too.
    pub fn remove_tag(&mut self, tag: u16) -> Vec<Field> {
        let (removed, kept): (Vec<Field>, Vec<Field>) = std::mem::take(&mut self.fields)
            .into_iter()
            .partition(|field| *field.tag() == tag);
        self.fields = kept;
        removed
    }
}
