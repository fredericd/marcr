use memchr::memchr2;

use crate::{parse_digits, Error, Field, Record, Subfield, DE, FT, RT};

use super::Format;

impl Format {
    /// Serializes `record` to ISO 2709: leader, directory, then fields
    /// terminated by FT (`0x1e`), the whole terminated by RT (`0x1d`).
    /// Recomputes the data length and offset in the leader.
    ///
    /// Returns an error if a tag exceeds 999, a field 9999 bytes or the
    /// record 99999 bytes, the limits of the 3-, 4- and 5-digit numbers of
    /// the format.
    pub fn serialize_iso2709(&self, record: &Record) -> Result<Vec<u8>, Error> {
        // First pass: sizes, so that the record is written in a single
        // buffer allocated once.
        let mut data_len = 0;
        for field in record.fields.iter() {
            let tag = *field.tag();
            if tag > 999 {
                return Err(Error::unwritable(Format::Iso2709, format!("invalid tag {tag} (max 999)")));
            }
            let len = field_len(field);
            if len > 9999 {
                return Err(Error::unwritable(Format::Iso2709, format!("field {tag:03} too long ({len} bytes, max 9999)")));
            }
            data_len += len;
        }
        let offset = 24 + 12 * record.fields.len() + 1;
        let length = offset + data_len + 1;
        if length > 99999 {
            return Err(Error::unwritable(Format::Iso2709, format!("record too long ({length} bytes, max 99999)")));
        }

        let mut data: Vec<u8> = Vec::with_capacity(length);
        let mut leader = record.leader;
        put_digits(&mut leader[..5], length);
        put_digits(&mut leader[12..17], offset);
        data.extend_from_slice(&leader);
        let mut from = 0;
        for field in record.fields.iter() {
            let len = field_len(field);
            let mut entry = [0u8; 12];
            put_digits(&mut entry[..3], *field.tag() as usize);
            put_digits(&mut entry[3..7], len);
            put_digits(&mut entry[7..], from);
            data.extend_from_slice(&entry);
            from += len;
        }
        data.push(FT);
        for field in record.fields.iter() {
            match field {
                Field::Control(_, value) => data.extend_from_slice(value.as_bytes()),
                Field::Standard(_, ind, subfields) => {
                    data.push(ind[0] as u8);
                    data.push(ind[1] as u8);
                    for Subfield(letter, value) in subfields {
                        data.push(DE);
                        data.push(*letter as u8);
                        data.extend_from_slice(value.as_bytes());
                    }
                }
            }
            data.push(FT);
        }
        data.push(RT);
        debug_assert_eq!(data.len(), length);
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
        // Validated once for the whole record (one call per field costs
        // more on short fields); per field only if the record is invalid.
        let data_text = std::str::from_utf8(data).ok();
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
            // Delimiters being ASCII, the subfields of a valid field are
            // valid UTF-8 too (`get` checks they fall on char boundaries).
            let text = match data_text {
                Some(data_text) => data_text.get(offset..offset + field_end),
                None => std::str::from_utf8(&field[..field_end]).ok(),
            };
            let to_string = |range: std::ops::Range<usize>| match text.and_then(|t| t.get(range.clone())) {
                Some(value) => value.to_owned(),
                None => String::from_utf8_lossy(&field[range]).into_owned(),
            };
            if tag < 10 {
                fields.push(Field::Control(tag, to_string(0..field_end)));
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
                        subfields.push(Subfield(letter, to_string(j..k)));
                        j = k;
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

/// Length in bytes of `field` in ISO 2709, its FT terminator included.
fn field_len(field: &Field) -> usize {
    match field {
        Field::Control(_, value) => value.len() + 1,
        Field::Standard(_, _, subfields) => {
            3 + subfields.iter().map(|Subfield(_, value)| 2 + value.len()).sum::<usize>()
        }
    }
}

/// Writes `n` as decimal digits, zero-padded to fill `dst`. Callers check
/// that `n` fits: a faster `write!(dst, "{n:0width$}")`.
fn put_digits(dst: &mut [u8], mut n: usize) {
    for digit in dst.iter_mut().rev() {
        *digit = b'0' + (n % 10) as u8;
        n /= 10;
    }
}
