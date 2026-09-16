use std::io::Write;

use memchr::memchr2;

use crate::{parse_digits, Field, Record, Subfield, DE, FT, RT};

use super::Format;

impl Format {
    /// Sérialise `record` en ISO 2709 : leader, répertoire, puis champs
    /// terminés par FT (`0x1e`), l'ensemble terminé par RT (`0x1d`).
    /// Recalcule la longueur et l'offset des données dans le leader.
    pub fn serialize_iso2709(&self, record: &Record) -> Vec<u8> {
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
            write!(directory, "{tag:03}{len:04}{from:05}").unwrap();
            from += len;
        }
        let offset = 24 + 12 * record.fields.len() + 1;
        let length = offset + from + 1;
        let mut leader = record.leader;
        write!(&mut leader[..5], "{length:05}").unwrap();
        write!(&mut leader[12..17], "{offset:05}").unwrap();
        directory.push(FT);
        fields.push(RT);
        let mut data: Vec<u8> = Vec::with_capacity(leader.len() + directory.len() + fields.len());
        data.extend_from_slice(&leader);
        data.extend_from_slice(&directory);
        data.extend_from_slice(&fields);
        data
    }

    /// Parse une notice ISO 2709 complète (leader + répertoire + champs
    /// terminés par FT/RT). Retourne une erreur si `octets` fait moins de
    /// 40 octets ou si le répertoire est mal formé.
    pub fn deserialize_iso2709(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        if octets.len() < 40 { return Err("Invalid record. Too short".into()); }
        let leader: [u8; 24] = octets[..24].try_into().unwrap();
        let directory_len = parse_digits(&octets[12..17])
            .ok_or("Bad ISO2709, invalid leader length")?;
        let number_of_tags = (directory_len - 24 - 1) / 12;
        let mut fields: Vec<Field> = Vec::with_capacity(number_of_tags);
        for i in 0..number_of_tags {
            let directory_offset = 24 + i * 12;
            let tag: u16 = parse_digits(&octets[directory_offset..directory_offset+3])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or("Bad ISO2709, invalid tag")?;
            let len: usize = parse_digits(&octets[directory_offset+3..directory_offset+3+4])
                .ok_or("Bad ISO2709, length non digit")? - 1;
            let offset: usize = parse_digits(&octets[directory_offset+3+4..directory_offset+3+4+5])
                .ok_or("Bad ISO2709, invalid offset")?;
            let base = directory_len + offset;
            if tag < 10 {
                let slice = &octets[base..base + len];
                let value = String::from_utf8_lossy(slice).into_owned();
                fields.push(Field::Control(tag, value));
            } else {
                let ind: [char; 2] = [octets[base] as char, octets[base+1] as char];
                let mut j = base + 2;
                let field_end = base + len;
                // +1 : la borne inclut l'octet FT qui termine le dernier sous-champ
                // (exclu de `field_end`, qui sert à arrêter la boucle *avant* ce FT).
                let scan_end = field_end + 1;
                let mut subfields: Vec<Subfield> = Vec::with_capacity(3);
                while j < field_end {
                    if octets[j] == DE {
                        j += 1;
                        let letter: char = octets[j] as char;
                        j += 1;
                        let k = memchr2(DE, FT, &octets[j..scan_end])
                            .map(|p| j + p)
                            .ok_or("Bad ISO2709, subfield not terminated")?;
                        let slice = &octets[j..k];
                        let value = String::from_utf8_lossy(slice).into_owned();
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
