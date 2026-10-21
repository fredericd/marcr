use crate::{parse_digits, Error, Field, Record, Subfield};

use super::Format;

/// Characters that cannot appear literally in MRK data, with their
/// MARCMaker mnemonic.
const MNEMONICS: [(char, &str); 4] = [
    ('$', "{dollar}"),
    ('\\', "{bsol}"),
    ('{', "{lcub}"),
    ('}', "{rcub}"),
];

/// Appends `value` to `buffer`, escaping special characters as mnemonics
/// and, if `blanks` (leader, control fields), spaces as `\`.
fn push_escaped(buffer: &mut String, value: &str, blanks: bool) {
    if !value.contains(['$', '\\', '{', '}']) && !(blanks && value.contains(' ')) {
        buffer.push_str(value); // Fast path: nothing to escape
        return;
    }
    for c in value.chars() {
        match MNEMONICS.iter().find(|(m, _)| *m == c) {
            Some((_, mnemonic)) => buffer.push_str(mnemonic),
            None if blanks && c == ' ' => buffer.push('\\'),
            None => buffer.push(c),
        }
    }
}

/// Reverses [`push_escaped`]. Unknown mnemonics are kept as is.
// shortcut: MARC-8 character mnemonics ({acute}, {eacute}...) are not
// decoded, add them if MARC-8 era .mrk files must be read.
fn unescape(value: &str, blanks: bool) -> String {
    if !value.contains(['{', '\\']) {
        return value.to_string(); // Fast path: nothing to unescape
    }
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(c) = rest.chars().next() {
        if let Some((plain, mnemonic)) = MNEMONICS.iter().find(|(_, m)| rest.starts_with(m)) {
            out.push(*plain);
            rest = &rest[mnemonic.len()..];
            continue;
        }
        out.push(if blanks && c == '\\' { ' ' } else { c });
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// A blank indicator is written `\`.
fn indicator(c: char) -> char {
    if c == ' ' { '\\' } else { c }
}

impl Format {
    /// Serializes `record` to the MARCMaker / MarcEdit mnemonic format
    /// (`.mrk`): `=LDR  leader`, then one line per field — `=001  value`
    /// for a control field, `=245  10$aTitle$bSubtitle` for a standard
    /// field. Blanks in the leader, control fields and indicators are
    /// written `\`; `$`, `\`, `{` and `}` in data as `{dollar}`, `{bsol}`,
    /// `{lcub}` and `{rcub}`.
    pub fn serialize_mrk(&self, record: &Record) -> Vec<u8> {
        let mut buffer = String::from("=LDR  ");
        push_escaped(&mut buffer, &String::from_utf8_lossy(&record.leader), true);
        buffer.push('\n');
        for field in record.fields.iter() {
            match field {
                Field::Control(tag, value) => {
                    buffer.push_str(&format!("={tag:03}  "));
                    push_escaped(&mut buffer, value, true);
                },
                Field::Standard(tag, ind, subfields) => {
                    buffer.push_str(&format!("={tag:03}  {}{}", indicator(ind[0]), indicator(ind[1])));
                    for Subfield(code, value) in subfields {
                        buffer.push('$');
                        buffer.push(*code);
                        push_escaped(&mut buffer, value, false);
                    }
                },
            }
            buffer.push('\n');
        }
        buffer.into_bytes()
    }

    /// Parses a record in the MARCMaker / MarcEdit mnemonic format (see
    /// [`Format::serialize_mrk`]). CRLF line endings and a leading UTF-8
    /// BOM, both common in MarcEdit files, are accepted.
    pub fn deserialize_mrk(&self, octets: &[u8]) -> Result<Record, Error> {
        let malformed = |message: &str| Error::malformed(Format::Mrk, message);
        let text = std::str::from_utf8(octets)
            .map_err(|e| Error::malformed(Format::Mrk, e.to_string()))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut lines = text.lines().filter(|line| !line.is_empty());

        let leader_line = lines.next().ok_or_else(|| malformed("missing leader"))?;
        let leader = leader_line.strip_prefix("=LDR  ").ok_or_else(|| malformed("missing =LDR line"))?;
        let leader: [u8; 24] = unescape(leader, true).as_bytes().try_into()
            .map_err(|_| malformed("invalid leader length"))?;

        let mut fields = Vec::new();
        for line in lines {
            let bytes = line.as_bytes();
            if bytes.len() < 6 || bytes[0] != b'=' || &bytes[4..6] != b"  " {
                return Err(malformed("a field line must start with =TAG and two spaces"));
            }
            let tag: u16 = parse_digits(&bytes[1..4])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or_else(|| malformed("invalid tag"))?;
            let data = &line[6..];
            if tag < 10 {
                fields.push(Field::Control(tag, unescape(data, true)));
                continue;
            }
            let mut chars = data.chars();
            let (Some(ind1), Some(ind2)) = (chars.next(), chars.next()) else {
                return Err(malformed("missing indicators"));
            };
            let blank = |c: char| if c == '\\' { ' ' } else { c };
            let mut subfields = Vec::new();
            let rest = chars.as_str();
            if !rest.is_empty() {
                let body = rest.strip_prefix('$').ok_or_else(|| malformed("malformed subfield"))?;
                for chunk in body.split('$') {
                    let mut chars = chunk.chars();
                    let code = chars.next().ok_or_else(|| malformed("missing subfield code"))?;
                    subfields.push(Subfield(code, unescape(chars.as_str(), false)));
                }
            }
            fields.push(Field::Standard(tag, [blank(ind1), blank(ind2)], subfields));
        }
        Ok(Record { leader, fields })
    }
}
