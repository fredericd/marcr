use std::io::Write;

use crate::{parse_digits, Error, Field, Record, Subfield};

use super::Format;

impl Format {
    /// Serializes `record` to human-readable text: the leader then one
    /// line per field — `tag value` (a single separator space) for a
    /// control field, `tag ind1ind2 $code value ...` for a standard field.
    pub fn serialize_text(&self, record: &Record) -> Vec<u8> {
        let mut buffer: Vec<u8> = Vec::new();
        let leader = String::from_utf8_lossy(&record.leader);
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
    pub fn deserialize_text(&self, octets: &[u8]) -> Result<Record, Error> {
        let text = std::str::from_utf8(octets)
            .map_err(|e| Error::malformed(Format::Text, e.to_string()))?;
        let mut lines = text.lines();

        let leader_line = lines.next().ok_or_else(|| Error::malformed(Format::Text, "missing leader"))?;
        let leader: [u8; 24] = leader_line.as_bytes().try_into()
            .map_err(|_| Error::malformed(Format::Text, "invalid leader length"))?;

        let mut fields = Vec::new();
        for line in lines {
            if line.is_empty() { continue; }
            if line.len() < 3 {
                return Err(Error::malformed(Format::Text, "field line too short"));
            }
            let bytes = line.as_bytes();
            let tag: u16 = parse_digits(&bytes[..3])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or_else(|| Error::malformed(Format::Text, "invalid tag"))?;

            if tag < 10 {
                // "tag value": a single separator space (no indicators
                // for a control field).
                if bytes.get(3) != Some(&b' ') {
                    return Err(Error::malformed(Format::Text, "missing separator after tag"));
                }
                fields.push(Field::Control(tag, line[4..].to_string()));
            } else {
                // "tag ind1ind2 $code value ...": fixed-width structure
                // (7 characters before the subfields), the two indicators
                // cannot act as a separator.
                if line.len() < 7 || !line.is_char_boundary(7) {
                    return Err(Error::malformed(Format::Text, "field line too short"));
                }
                let ind: [char; 2] = [bytes[4] as char, bytes[5] as char];
                let rest = &line[7..];
                let mut subfields = Vec::new();
                if !rest.is_empty() {
                    let body = rest.strip_prefix('$')
                        .ok_or_else(|| Error::malformed(Format::Text, "malformed subfield"))?;
                    for chunk in body.split(" $") {
                        let mut chars = chunk.chars();
                        let code = chars.next()
                            .ok_or_else(|| Error::malformed(Format::Text, "missing subfield code"))?;
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
