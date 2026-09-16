use std::io::Write;
use std::str;

use crate::{parse_digits, Field, Record, Subfield};

use super::Format;

impl Format {
    /// Implémentation de référence, non optimisée, de la sérialisation
    /// texte (assemble des `String` intermédiaires). Conservée à titre de
    /// comparaison ; [`Format::serialize`] utilise [`Format::serialize_text`].
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

    /// Sérialise `record` en texte lisible : le leader puis une ligne par
    /// champ — `tag valeur` (un seul espace séparateur) pour un champ de
    /// contrôle, `tag ind1ind2 $code valeur ...` pour un champ standard.
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

    /// Parse un enregistrement produit par [`Format::serialize_text`] :
    /// une ligne pour le leader (24 caractères), puis une ligne par champ —
    /// `tag valeur` (un seul espace séparateur) pour un champ de contrôle
    /// (tag < 10), ou `tag ind1ind2 $code valeur $code2 valeur2 ...`
    /// (structure à largeur fixe) pour un champ standard.
    ///
    /// Le format texte n'a pas de mécanisme d'échappement : une valeur de
    /// sous-champ contenant littéralement `" $"` suivi d'un caractère sera
    /// mal interprétée comme le début d'un nouveau sous-champ.
    pub fn deserialize_text(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        let text = std::str::from_utf8(octets)?;
        let mut lines = text.lines();

        let leader_line = lines.next().ok_or("Bad text record: leader manquant")?;
        let leader: [u8; 24] = leader_line.as_bytes().try_into()
            .map_err(|_| "Bad text record: leader de longueur invalide")?;

        let mut fields = Vec::new();
        for line in lines {
            if line.is_empty() { continue; }
            if line.len() < 3 {
                return Err("Bad text record: ligne de champ trop courte".into());
            }
            let bytes = line.as_bytes();
            let tag: u16 = parse_digits(&bytes[..3])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or("Bad text record: tag invalide")?;

            if tag < 10 {
                // "tag valeur" : un seul espace séparateur (pas
                // d'indicateurs pour un champ de contrôle).
                if bytes.get(3) != Some(&b' ') {
                    return Err("Bad text record: séparateur manquant après le tag".into());
                }
                fields.push(Field::Control(tag, line[4..].to_string()));
            } else {
                // "tag ind1ind2 $code valeur ...": structure à largeur
                // fixe (7 caractères avant les sous-champs), les deux
                // indicateurs ne pouvant pas servir de séparateur.
                if line.len() < 7 || !line.is_char_boundary(7) {
                    return Err("Bad text record: ligne de champ trop courte".into());
                }
                let ind: [char; 2] = [bytes[4] as char, bytes[5] as char];
                let rest = &line[7..];
                let mut subfields = Vec::new();
                if !rest.is_empty() {
                    let body = rest.strip_prefix('$')
                        .ok_or("Bad text record: sous-champ mal formé")?;
                    for chunk in body.split(" $") {
                        let mut chars = chunk.chars();
                        let code = chars.next()
                            .ok_or("Bad text record: code de sous-champ manquant")?;
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
