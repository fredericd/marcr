use serde_json::Value;

use crate::{Error, Field, Record, Subfield};

use super::Format;

/// Appends `s` to `buffer` as a JSON string literal (quoted, escaped).
fn push_str(buffer: &mut Vec<u8>, s: &str) {
    serde_json::to_writer(&mut *buffer, s).unwrap(); // Writing to a Vec cannot fail
}

impl Format {
    /// Serializes `record` to a MARC-in-JSON object, on a single line:
    /// `{"leader":"...","fields":[{"001":"..."},{"245":{"ind1":" ","ind2":"0","subfields":[{"a":"..."}]}}]}`.
    pub fn serialize_json(&self, record: &Record) -> Vec<u8> {
        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(b"{\"leader\":");
        push_str(&mut buffer, &String::from_utf8_lossy(&record.leader));
        buffer.extend_from_slice(b",\"fields\":[");
        for (i, field) in record.fields.iter().enumerate() {
            if i > 0 {
                buffer.push(b',');
            }
            match field {
                Field::Control(tag, value) => {
                    buffer.extend_from_slice(format!("{{\"{tag:03}\":").as_bytes());
                    push_str(&mut buffer, value);
                },
                Field::Standard(tag, ind, subfields) => {
                    buffer.extend_from_slice(format!("{{\"{tag:03}\":{{\"ind1\":").as_bytes());
                    push_str(&mut buffer, ind[0].encode_utf8(&mut [0; 4]));
                    buffer.extend_from_slice(b",\"ind2\":");
                    push_str(&mut buffer, ind[1].encode_utf8(&mut [0; 4]));
                    buffer.extend_from_slice(b",\"subfields\":[");
                    for (j, Subfield(code, value)) in subfields.iter().enumerate() {
                        if j > 0 {
                            buffer.push(b',');
                        }
                        buffer.push(b'{');
                        push_str(&mut buffer, code.encode_utf8(&mut [0; 4]));
                        buffer.push(b':');
                        push_str(&mut buffer, value);
                        buffer.push(b'}');
                    }
                    buffer.extend_from_slice(b"]}");
                },
            }
            buffer.push(b'}');
        }
        buffer.extend_from_slice(b"]}");
        buffer
    }

    /// Parses a MARC-in-JSON object (see [`Format::serialize_json`]).
    /// Unknown keys are ignored; a missing indicator is read as a space.
    pub fn deserialize_json(&self, octets: &[u8]) -> Result<Record, Error> {
        parse_json(octets).map_err(|e| Error::malformed(Format::Json, e))
    }
}

/// Parsing of [`Format::deserialize_json`], errors as plain messages.
fn parse_json(octets: &[u8]) -> Result<Record, String> {
    let json: Value = serde_json::from_slice(octets).map_err(|e| e.to_string())?;
    let leader: [u8; 24] = json["leader"].as_str()
        .ok_or("missing leader")?
        .as_bytes().try_into()
        .map_err(|_| "invalid leader length")?;
    let mut fields = Vec::new();
    for field in json["fields"].as_array().ok_or("missing fields")? {
        let (tag, content) = field.as_object()
            .filter(|object| object.len() == 1)
            .and_then(|object| object.iter().next())
            .ok_or("a field must be an object with a single tag key")?;
        let tag: u16 = tag.parse().map_err(|_| format!("invalid tag {tag:?}"))?;
        if let Some(value) = content.as_str() {
            fields.push(Field::Control(tag, value.to_string()));
            continue;
        }
        let indicator = |key: &str| content[key].as_str().and_then(|s| s.chars().next()).unwrap_or(' ');
        let mut subfields = Vec::new();
        for subfield in content["subfields"].as_array().ok_or(format!("field {tag:03}: missing subfields"))? {
            let (code, value) = subfield.as_object()
                .filter(|object| object.len() == 1)
                .and_then(|object| object.iter().next())
                .and_then(|(code, value)| Some((code.chars().next()?, value.as_str()?)))
                .ok_or(format!("field {tag:03}: a subfield must be an object with a single code key and a string value"))?;
            subfields.push(Subfield(code, value.to_string()));
        }
        fields.push(Field::Standard(tag, [indicator("ind1"), indicator("ind2")], subfields));
    }
    Ok(Record { leader, fields })
}
