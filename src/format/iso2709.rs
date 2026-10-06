use std::io::Write;

use memchr::memchr2;

use crate::{parse_digits, Error, Field, Record, Subfield, DE, FT, RT};

use super::Format;

impl Format {
    /// Serializes `record` to ISO 2709: leader, directory, then fields
    /// terminated by FT (`0x1e`), the whole terminated by RT (`0x1d`).
    /// Recomputes the data length and offset in the leader.
    ///
    /// Returns an error if a field exceeds 9999 bytes or the record 99999
    /// bytes, the limits of the 4- and 5-digit lengths of the format.
    pub fn serialize_iso2709(&self, record: &Record) -> Result<Vec<u8>, Error> {
        let mut fields: Vec<u8> = Vec::new();
        let mut directory: Vec<u8> = Vec::with_capacity(record.fields.len() * 12 + 1);
        let mut from = 0;
        for field in record.fields.iter() {
            let start = fields.len();
            let tag = match field {
                Field::Control(tag, value) => {
                    fields.extend_from_slice(value.as_bytes());
                    tag
                },
                Field::Standard(tag, ind, subfields) => {
                    fields.push(ind[0] as u8);
                    fields.push(ind[1] as u8);
                    for Subfield(letter, value) in subfields {
                        fields.push(DE);
                        fields.push(*letter as u8);
                        fields.extend_from_slice(value.as_bytes());
                    }
                    tag
                }
            };
            fields.push(FT);
            let len = fields.len() - start;
            if len > 9999 {
                return Err(Error::unwritable(Format::Iso2709, format!("field {tag:03} too long ({len} bytes, max 9999)")));
            }
            write!(directory, "{tag:03}{len:04}{from:05}")?;
            from += len;
        }
        let offset = 24 + 12 * record.fields.len() + 1;
        let length = offset + from + 1;
        if length > 99999 {
            return Err(Error::unwritable(Format::Iso2709, format!("record too long ({length} bytes, max 99999)")));
        }
        let mut leader = record.leader;
        write!(&mut leader[..5], "{length:05}")?;
        write!(&mut leader[12..17], "{offset:05}")?;
        directory.push(FT);
        fields.push(RT);
        let mut data: Vec<u8> = Vec::with_capacity(leader.len() + directory.len() + fields.len());
        data.extend_from_slice(&leader);
        data.extend_from_slice(&directory);
        data.extend_from_slice(&fields);
        Ok(data)
    }

    /// Parses a complete ISO 2709 record (leader + directory + fields
    /// terminated by FT/RT). Returns an error, never panics, on malformed
    /// input: record too short, non-ASCII leader, invalid directory, or a
    /// field lying outside the record.
    pub fn deserialize_iso2709(&self, octets: &[u8]) -> Result<Record, Error> {
        if octets.len() < 40 { return Err(Error::malformed(Format::Iso2709, "record too short")); }
        let leader: [u8; 24] = octets[..24].try_into()
            .map_err(|_| Error::malformed(Format::Iso2709, "record too short"))?;
        if !leader.is_ascii() {
            return Err(Error::malformed(Format::Iso2709, "non-ASCII leader"));
        }
        let base_address = parse_digits(&octets[12..17])
            .ok_or_else(|| Error::malformed(Format::Iso2709, "invalid leader length"))?;
        // The directory runs from byte 24 up to the FT preceding the data.
        if base_address < 25 || base_address > octets.len() {
            return Err(Error::malformed(Format::Iso2709, "base address outside the record"));
        }
        let directory = &octets[24..base_address - 1];
        let data = &octets[base_address..];
        let mut fields: Vec<Field> = Vec::with_capacity(directory.len() / 12);
        for entry in directory.chunks_exact(12) {
            let tag: u16 = parse_digits(&entry[0..3])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or_else(|| Error::malformed(Format::Iso2709, "invalid tag"))?;
            let len: usize = parse_digits(&entry[3..7])
                .ok_or_else(|| Error::malformed(Format::Iso2709, "length non digit"))?;
            let offset: usize = parse_digits(&entry[7..12])
                .ok_or_else(|| Error::malformed(Format::Iso2709, "invalid offset"))?;
            // The field with its terminating FT; `len` includes that FT.
            let field = data.get(offset..offset + len)
                .filter(|field| !field.is_empty())
                .ok_or_else(|| Error::malformed(Format::Iso2709, "field outside the record"))?;
            let field_end = len - 1; // FT excluded
            if tag < 10 {
                let value = String::from_utf8_lossy(&field[..field_end]).into_owned();
                fields.push(Field::Control(tag, value));
            } else {
                if field_end < 2 {
                    return Err(Error::malformed(Format::Iso2709, "missing indicators"));
                }
                let ind: [char; 2] = [field[0] as char, field[1] as char];
                let mut j = 2;
                let mut subfields: Vec<Subfield> = Vec::with_capacity(3);
                while j < field_end {
                    if field[j] == DE {
                        // j + 1 <= field_end < field.len(): always in bounds
                        let letter: char = field[j + 1] as char;
                        j += 2;
                        // The search includes the FT terminating the last
                        // subfield.
                        let k = memchr2(DE, FT, &field[j..])
                            .map(|p| j + p)
                            .ok_or_else(|| Error::malformed(Format::Iso2709, "subfield not terminated"))?;
                        let value = String::from_utf8_lossy(&field[j..k]).into_owned();
                        j = k;
                        subfields.push(Subfield(letter, value));
                    }
                    else {
                        j += 1;
                    }
                }
                fields.push(Field::Standard(tag, ind, subfields));
            }
        }
        let record = Record {
            leader,
            fields,
        };
        Ok(record)
    }
}
