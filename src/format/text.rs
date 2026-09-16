use std::io::Write;
use std::str;

use crate::{parse_digits, Field, Record, Subfield};

use super::Format;

impl Format {
    /// Reference, unoptimized implementation of text serialization
    /// (assembles intermediate `String`s). Kept for comparison;
    /// [`Format::serialize`] uses [`Format::serialize_text`].
    pub fn serialize_text_slow(&self, record: &Record) -> Vec<u8> {
        let mut lines: Vec<String> = Vec::new();
        let leader = unsafe { str::from_utf8_unchecked(&record.leader) };
        lines.push(leader.to_string());
        for field in record.fields.iter() {
            match field {
                Field::Control(tag, value) => {
                    lines.push(format!("{tag:03} {value}"));
                },
                Field::Standard(tag, ind, subfields) => {
                    let ind1 = ind[0];
                    let ind2 = ind[1];
                    let concatenated = subfields
                        .iter()
                        .map(|s| s.to_string())
                        .collect::<Vec<_>>()
                        .join(" ");
                    lines.push(format!("{tag:03} {ind1}{ind2} {concatenated}"));
                }
            }
        }
        lines.push("\n".to_string());
        lines.join("\n").into_bytes()
    }

    /// Serializes `record` to human-readable text: the leader then one
    /// line per field — `tag value` (a single separator space) for a
    /// control field, `tag ind1ind2 $code value ...` for a standard field.
    pub fn serialize_text(&self, record: &Record) -> Vec<u8> {
        let mut buffer: Vec<u8> = Vec::new();
        let leader = unsafe { str::from_utf8_unchecked(&record.leader) };
        write!(buffer, "{}\n", leader).unwrap();
        for field in record.fields.iter() {
            match field {
                Field::Control(tag, value) => {
                    write!(buffer, "{tag:03} {value}\n").unwrap();
                },
                Field::Standard(tag, ind, subfields) => {
                    let ind1 = ind[0];
                    let ind2 = ind[1];
                    write!(buffer, "{tag:03} {ind1}{ind2} ").unwrap();
                    let mut first = true;
                    subfields.iter().for_each(|Subfield(l,v)| {
                        if first {
                            write!(buffer, "${l} {v}").unwrap();
                            first = false;
                        } else {
                            write!(buffer, " ${l} {v}").unwrap();
                        }
                    });
                    write!(buffer, "\n").unwrap();
                }
            }
        }
        buffer
    }

    /// Parses a record produced by [`Format::serialize_text`]: one line
    /// for the leader (24 characters), then one line per field — `tag
    /// value` (a single separator space) for a control field (tag < 10),
    /// or `tag ind1ind2 $code value $code2 value2 ...` (fixed-width
    /// structure) for a standard field.
    ///
    /// The text format has no escaping mechanism: a subfield value
    /// literally containing `" $"` followed by a character will be
    /// misinterpreted as the start of a new subfield.
    pub fn deserialize_text(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        let text = std::str::from_utf8(octets)?;
        let mut lines = text.lines();

        let leader_line = lines.next().ok_or("Bad text record: missing leader")?;
        let leader: [u8; 24] = leader_line.as_bytes().try_into()
            .map_err(|_| "Bad text record: invalid leader length")?;

        let mut fields = Vec::new();
        for line in lines {
            if line.is_empty() { continue; }
            if line.len() < 3 {
                return Err("Bad text record: field line too short".into());
            }
            let bytes = line.as_bytes();
            let tag: u16 = parse_digits(&bytes[..3])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or("Bad text record: invalid tag")?;

            if tag < 10 {
                // "tag value": a single separator space (no indicators
                // for a control field).
                if bytes.get(3) != Some(&b' ') {
                    return Err("Bad text record: missing separator after tag".into());
                }
                fields.push(Field::Control(tag, line[4..].to_string()));
            } else {
                // "tag ind1ind2 $code value ...": fixed-width structure
                // (7 characters before the subfields), the two indicators
                // cannot act as a separator.
                if line.len() < 7 || !line.is_char_boundary(7) {
                    return Err("Bad text record: field line too short".into());
                }
                let ind: [char; 2] = [bytes[4] as char, bytes[5] as char];
                let rest = &line[7..];
                let mut subfields = Vec::new();
                if !rest.is_empty() {
                    let body = rest.strip_prefix('$')
                        .ok_or("Bad text record: malformed subfield")?;
                    for chunk in body.split(" $") {
                        let mut chars = chunk.chars();
                        let code = chars.next()
                            .ok_or("Bad text record: missing subfield code")?;
                        let value = chars.as_str().strip_prefix(' ').unwrap_or(chars.as_str());
                        subfields.push(Subfield(code, value.to_string()));
                    }
                }
                fields.push(Field::Standard(tag, ind, subfields));
            }
        }
        Ok(Record { leader, fields })
    }
}
