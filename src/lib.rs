//! Lecture et écriture de notices bibliographiques MARC dans plusieurs
//! formats : ISO 2709 ([`Format::Iso2709`]), MARCXML ([`Format::Marcxml`])
//! et un format texte lisible, disponible en sortie uniquement
//! ([`Format::Text`]).
//!
//! [`Record`] représente une notice (leader + champs), [`Reader`] la lit
//! depuis un flux quel que soit le format, [`Writer`] l'écrit vers un
//! flux. [`Format`] centralise la sérialisation/désérialisation propre à
//! chaque format.
//!
//! ```no_run
//! use marcr::{Format, Reader, Writer};
//! use std::io::BufReader;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let input = BufReader::new(std::fs::File::open("notices.mrc")?);
//! let mut reader = Reader::new(Format::Iso2709, input);
//! let mut writer = Writer::new(Format::Marcxml, std::io::stdout());
//! while let Some(record) = reader.read()? {
//!     writer.write(&record)?;
//! }
//! # Ok(())
//! # }
//! ```

use std::fmt;
use std::str;
use std::io::{Write, Read, BufRead};
use quick_xml::events::{BytesText, Event};
use quick_xml::writer::Writer as XmlWriter;
use quick_xml::reader::Reader as XmlReader;
use memchr::{memchr, memchr2};

const FT: u8 = 0x1e; // Field terminator
const RT: u8 = 0x1d; // Record terminator
const DE: u8 = 0x1f; // Delimiter
const DEFAULT_LEADER: [u8; 24] = *b"00000nam a2200000   4500";
const XML_START_PREFIX: &[u8] = b"<record"; // sans '>' : la balise peut porter des attributs (xmlns, ...)
const XML_END_TAG: &[u8] = b"</record>";

/// Parse un nombre décimal ASCII de largeur fixe (répertoire ISO2709), sans
/// passer par la validation UTF-8 générique + FromStr de `str::parse`.
fn parse_digits(bytes: &[u8]) -> Option<usize> {
    let mut n: usize = 0;
    for &b in bytes {
        if !b.is_ascii_digit() { return None; }
        n = n * 10 + (b - b'0') as usize;
    }
    Some(n)
}

/// Un sous-champ d'un champ MARC "standard" (tag ≥ 10) : un code d'un
/// caractère (`$a`, `$b`, ...) et sa valeur.
#[derive(Debug)]
pub struct Subfield(pub char, pub String);

impl fmt::Display for Subfield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Subfield(letter, value) = &self;
        write!(f, "${} {}", *letter as char, value)
    }
}

/// Un champ d'une notice MARC : soit un champ de contrôle (tag < 10, une
/// simple valeur texte), soit un champ standard (tag ≥ 10, deux
/// indicateurs et une liste de sous-champs).
#[derive(Debug)]
pub enum Field {
    /// Champ de contrôle (tags 001-009) : tag et valeur.
    Control(u16, String),
    /// Champ standard (tags ≥ 010) : tag, les deux indicateurs, et les
    /// sous-champs.
    Standard(u16, [char; 2], Vec<Subfield>),
}

impl Field {
    /// Le tag du champ (001-999).
    pub fn tag(&self) -> &u16 {
        match self {
            Field::Control(tag, _) => tag,
            Field::Standard(tag, _, _) => tag,
        }
    }
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::Control(tag, value) => write!(f, "{tag:03}    {value}"),
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

/// Une notice bibliographique MARC : un leader de 24 octets et une liste
/// de champs.
#[derive(Debug)]
pub struct Record {
    /// Les 24 octets du leader (longueur de la notice, statut, type de
    /// document, position/taille du répertoire, etc.).
    pub leader: [u8; 24],
    /// Les champs de la notice, triés par tag croissant lorsqu'ils sont
    /// ajoutés via [`Record::add`] ou [`Record::insert`].
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
    /// Insère `field` en conservant l'ordre croissant des tags (tri par
    /// insertion : les champs de même tag gardent leur ordre relatif).
    pub fn add(&mut self, field: Field) {
        let tag = field.tag();
        let pos: usize = self.fields.iter()
            .position(|item| item.tag() > tag)
            .unwrap_or(self.fields.len());
        self.fields.insert(pos, field);
    }

    /// Construit une notice avec le leader par défaut et les champs
    /// donnés, sans les trier (contrairement à [`Record::add`]).
    pub fn new(fields: Vec<Field>) -> Self {
        let leader: [u8; 24] = DEFAULT_LEADER;
        Self {
            leader,
            fields,
        }
    }

    /// Construit et ajoute des champs à partir d'une notation compacte :
    /// chaque `Vec<&str>` est `[tag, indicateurs_ou_valeur, code1, valeur1,
    /// code2, valeur2, ...]`.
    ///
    /// - Pour un champ de contrôle (tag ≤ 9) : `vec!["001", "PPN1234"]`.
    /// - Pour un champ standard : `vec!["200", "  ", "a", "Titre", "b", "Sous-titre"]`
    ///   où `"  "` sont les deux indicateurs.
    ///
    /// Les entrées trop courtes ou mal formées sont silencieusement
    /// ignorées. Les champs sont ajoutés triés par tag via [`Record::add`].
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
                        iter.next().expect("Premier caractère manquant"),
                        iter.next().expect("Deuxième caractère manquant"),
                    ];
                    let mut subfields: Vec<Subfield> = Vec::new();
                    for i in (2..len-1).step_by(2) {
                        let letter_str = a_a[i];
                        let value_str = a_a[i+1];
                        if letter_str.len() >= 1 && value_str.len() > 1 {
                            let mut iter = letter_str.chars();
                            let letter = iter.next().expect("Pas une lettre pour sous-champ");
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
}

/// Un format de notice MARC, utilisé pour la (dé)sérialisation via
/// [`Format::serialize`]/[`Format::deserialize`] ou par [`Reader`]/[`Writer`].
#[derive(PartialEq)]
pub enum Format {
  /// Format d'échange binaire ISO 2709 (extension `.mrc`). Lecture et
  /// écriture supportées.
  Iso2709,
  /// MARCXML, le schéma XML de la Library of Congress (extension `.xml`).
  /// Lecture et écriture supportées.
  Marcxml,
  /// Format texte lisible, une ligne par champ (extension `.txt`).
  /// [`Format::serialize`]/[`Format::deserialize`] sont supportés, mais
  /// pas [`Reader`] (aucune détection de limite d'enregistrement dans un
  /// flux de plusieurs notices).
  Text,
}

/// Métadonnées descriptives d'un [`Format`], utilisées par exemple pour
/// peupler une liste déroulante ou choisir une extension de fichier.
#[allow(dead_code)]
pub struct RWDescription {
    pub format: Format,
    pub description: String,
    pub extension: String,
}

impl Format {
    /// La liste des formats supportés avec leur description et leur
    /// extension de fichier usuelle.
    pub fn get_available_formats() -> Vec<RWDescription> {
        vec![
            RWDescription{
                format: Format::Iso2709,
                description: String::from("ISO 2709"),
                extension: String::from("mrc"),
            },
            RWDescription{
                format: Format::Marcxml,
                description: String::from("Marc XML"),
                extension: String::from("xml"),
            },
            RWDescription{
                format: Format::Text,
                description: String::from("Text"),
                extension: String::from("txt"),
            },
        ]
    }

    /// Parse `octets` (une notice complète, sans octets superflus avant
    /// ou après) selon `self`. Voir [`Reader`] pour lire des notices en
    /// série depuis un flux plus large.
    ///
    /// Retourne une erreur si `octets` n'est pas une notice valide dans ce
    /// format, ou toujours une erreur pour [`Format::Text`] (écriture
    /// uniquement).
    pub fn deserialize(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        match self {
            Format::Iso2709 => self.deserialize_iso2709(octets),
            Format::Marcxml => self.deserialize_marcxml(octets),
            Format::Text => self.deserialize_text(octets),
        }
    }

    /// Sérialise `record` selon `self`. Pour écrire plusieurs notices vers
    /// un flux (en-tête/pied MARCXML, séparateurs), préférer [`Writer`].
    pub fn serialize(&self, record: &Record) -> Vec<u8> {
        match self {
            Format::Iso2709 => self.serialize_iso2709(record),
            Format::Marcxml => self.serialize_marcxml(record),
            Format::Text => self.serialize_text(record),
        }
    }

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

    /// Sérialise `record` en un élément `<record>` MARCXML (indenté),
    /// sans prologue XML ni élément englobant `<collection>` — voir
    /// [`Writer`] pour produire un document MARCXML complet.
    pub fn serialize_marcxml(&self, record: &Record) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut xml_writer = XmlWriter::new_with_indent(cursor, b' ', 2);
        match xml_writer
            .create_element("record")
            .write_inner_content(|writer| {
                let leader_str: &str = unsafe { str::from_utf8_unchecked(&record.leader) };
                writer
                    .create_element("leader")
                    .write_text_content(BytesText::new(leader_str))?;
                for field in record.fields.iter() {
                    match field {
                        Field::Control(tag, value) => {
                            let tag_str = format!("{tag:03}");
                            writer
                                .create_element("controlfield")
                                .with_attribute(("tag", tag_str.as_str()))
                                .write_text_content(BytesText::new(value))?;
                        },
                        Field::Standard(tag, ind, subfields) => {
                            let tag_str = format!("{tag:03}");
                            writer
                                .create_element("datafield")
                                .with_attribute(("tag", tag_str.as_str()))
                                .with_attribute(("ind1", ind[0].to_string().as_str()))
                                .with_attribute(("ind2", ind[1].to_string().as_str()))
                                .write_inner_content(|w| {
                                    for Subfield(letter, value) in subfields {
                                        let letter_str = format!("{letter}");
                                        w
                                            .create_element("subfield")
                                            .with_attribute(("code", letter_str.as_str()))
                                            .write_text_content(BytesText::new(value))?;
                                    }
                                    Ok(())
                                })?;
                        },
                    }
                }
                Ok(())
            }) {
                Ok(_) => 1,
                Err(_) => 1, // No error possible!
            };
        let cursor = xml_writer.into_inner();
        let octets: Vec<u8> = cursor.into_inner();
        octets
    }

    /// Parse un élément `<record>` MARCXML (attributs et espace de noms
    /// ignorés ; seuls les éléments `leader`, `controlfield`, `datafield`
    /// et `subfield` sont reconnus). `octets` doit être de l'UTF-8 valide.
    pub fn deserialize_marcxml(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        let xml: &str = unsafe { str::from_utf8_unchecked(octets) };
        let mut reader = XmlReader::from_str(xml);
        let mut record = Record::default();
        let mut field: Option<Field> = None;
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(e) if e.name().as_ref() == b"record" => {
                    record = Record::default()
                }
                Event::Start(e) if e.name().as_ref() == b"leader" => {
                    let contenu = reader.read_text(e.name())?.decode()?.to_string();
                    let leader = contenu.as_bytes();
                    if leader.len() == 24 {
                        record.leader[..24].copy_from_slice(&leader[..24]);
                    }
                },
                Event::Start(e) if e.name().as_ref() == b"controlfield" => {
                    let tag = e.attributes()
                        .flatten()
                        .find(|attr| attr.key.as_ref() == b"tag")
                        .map(|attr| String::from_utf8_lossy(&attr.value).into_owned());
                    if let Some(t) = tag {
                        let contenu = reader.read_text(e.name())?.decode()?.to_string();
                        let tt: u16 = t.parse().unwrap();
                        let cf = Field::Control(tt, contenu);
                        record.fields.push(cf);
                    }
                },
                Event::Start(e) if e.name().as_ref() == b"datafield" => {
                    // Extraction : tag, ind1, ind2
                    let mut tag: Option<u16> = None;
                    let mut ind1 = None;
                    let mut ind2 = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"tag" => tag = Some(String::from_utf8_lossy(&attr.value).into_owned().parse().unwrap()),
                            b"ind1" => ind1 = Some(String::from_utf8_lossy(&attr.value).into_owned()),
                            b"ind2" => ind2 = Some(String::from_utf8_lossy(&attr.value).into_owned()),
                            _ => ()
                        }
                    }
                    if let Some(tag) = tag {
                        let i1 = ind1.as_deref().unwrap_or(" ").chars().next().unwrap_or(' ');
                        let i2 = ind2.as_deref().unwrap_or(" ").chars().next().unwrap_or(' ');
                        let subfields = Vec::new();
                        field = Some(Field::Standard(tag, [i1, i2], subfields));
                    }
                }
                Event::Start(e) if e.name().as_ref() == b"subfield" => {
                    // Extraction : code
                    let mut code = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"code" => code = Some(String::from_utf8_lossy(&attr.value).into_owned()),
                            _ => (),
                        };
                    }
                    if let Some(code) = code {
                        let contenu = reader.read_text(e.name())?.decode()?.to_string();
                        let letter = code.chars().next().unwrap_or(' ');
                        let subfield = Subfield(letter, contenu);
                        if let Some(f) = field.as_mut() {
                            match f {
                                Field::Standard(_, _, subfields) => {
                                    subfields.push(subfield);
                                },
                                _ => (),
                            };
                        }
                    }
                },
                Event::End(e) if e.name().as_ref() == b"datafield" => {
                    if let Some(f) = field.take() {
                        record.fields.push(f);
                    }
                },
                Event::Eof => break,
                _ => (),
            }
            buf.clear();
        }
        Ok(record)
    }

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
                    lines.push(format!("{tag:03}    {value}"));
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
    /// champ (`tag ind1ind2 $code valeur ...`).
    pub fn serialize_text(&self, record: &Record) -> Vec<u8> {
        let mut buffer: Vec<u8> = Vec::new();
        let leader = unsafe { str::from_utf8_unchecked(&record.leader) };
        write!(buffer, "{}\n", leader).unwrap();
        for field in record.fields.iter() {
            match field {
                Field::Control(tag, value) => {
                    write!(buffer, "{tag:03}    {value}\n").unwrap();
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
    /// `tag    valeur` pour un champ de contrôle (tag < 10), ou
    /// `tag ind1ind2 $code valeur $code2 valeur2 ...` pour un champ
    /// standard.
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
            if line.len() < 7 || !line.is_char_boundary(7) {
                return Err("Bad text record: ligne de champ trop courte".into());
            }
            let bytes = line.as_bytes();
            let tag: u16 = parse_digits(&bytes[..3])
                .and_then(|t| u16::try_from(t).ok())
                .ok_or("Bad text record: tag invalide")?;
            let rest = &line[7..];

            if tag < 10 {
                fields.push(Field::Control(tag, rest.to_string()));
            } else {
                let ind: [char; 2] = [bytes[4] as char, bytes[5] as char];
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

/// Lit des notices en série depuis un flux `R`, dans un format donné.
///
/// Chaque appel à [`Reader::read`] extrait et parse la notice suivante,
/// sans avoir à charger tout le flux en mémoire. `format: Text` n'est pas
/// supporté en lecture.
pub struct Reader<R> {
    pub format: Format,
    pub reader: R,
    pub buffer: Vec<u8>,   // Reusable internal buffer
    pub count: usize,
}

impl<R: Read + BufRead> Reader<R> {
    /// Crée un lecteur pour `format` au-dessus du flux bufferisé `reader`.
    pub fn new(format: Format, reader: R) -> Self {
        let buffer: Vec<u8> = Vec::new();
        let count = 0;
        Self { format, reader, buffer, count }
    }

    /// Lit et parse la prochaine notice du flux.
    ///
    /// Retourne `Ok(None)` en fin de flux, `Err` en cas d'erreur de
    /// lecture ou de notice mal formée, ou toujours `Err` si
    /// `format == Format::Text` (lecture non supportée).
    pub fn read(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        match self.format {
            Format::Iso2709 => self.read_iso2709(),
            Format::Marcxml => self.read_marcxml(),
            Format::Text    => return Err("Pas de parser pour Text".into()),
        }
    }

    /// Lit la prochaine notice ISO 2709 : cherche l'octet RT (`0x1d`) qui
    /// la termine dans le flux, en accumulant dans un tampon interne si
    /// elle s'étend sur plusieurs lectures du `BufReader`.
    pub fn read_iso2709(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        // Tampon de secours uniquement pour les notices plus grandes que la taille du BufReader
        self.buffer.clear();
        let mut found = false;
        let mut option_record: Option<Record> = None;
        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // End of reader without finding RT. Return None
                break;
            }
            let consumed = {
                if let Some(pos) = memchr(RT, available) {
                    if self.buffer.is_empty() {
                        // Found in reader buffer. No need to use an internal buffer, ie zero-copy
                        let octets = &available[..=pos];
                        option_record = Some(self.format.deserialize(octets)?);
                    } else {
                        // Found a record which was extended on several reader buffer
                        self.buffer.extend_from_slice(&available[..=pos]);
                        option_record = Some(self.format.deserialize(&self.buffer)?);
                    }
                    found = true;
                    pos + 1
                } else {
                    self.buffer.extend_from_slice(available);
                    available.len()
                }
            };
            self.reader.consume(consumed);
        }
        Ok(option_record)
    }

    /// Lit la prochaine notice MARCXML : cherche l'élément `<record>`
    /// suivant (avec ou sans attributs, ex. `xmlns="..."`) puis capture
    /// jusqu'à `</record>` inclus.
    pub fn read_marcxml(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        self.buffer.clear();
        let mut found = false;
        let mut option_record: Option<Record> = None;

        // Repérage de <record> : on matche le préfixe "<record", puis on
        // vérifie que l'octet suivant est bien une limite de nom de balise
        // (espace/tab/EOL ou '>'), afin d'accepter <record>, <record ...>
        // ou <record xmlns="...">, sans matcher par erreur <records>.
        enum State { Searching, OpenTag, Content }
        let mut state = State::Searching;
        let mut matched_start = 0; // progression dans XML_START_PREFIX
        let mut matched_end = 0;   // progression dans XML_END_TAG

        while !found {
            // 1. Accès direct au tampon mémoire du BufReader
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                break; // Fin de fichier propre
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                match state {
                    State::Searching => {
                        if b == XML_START_PREFIX[matched_start] {
                            matched_start += 1;
                            if matched_start == XML_START_PREFIX.len() {
                                self.buffer.extend_from_slice(XML_START_PREFIX);
                                state = State::OpenTag;
                            }
                        } else if b == XML_START_PREFIX[0] {
                            matched_start = 1;
                        } else {
                            matched_start = 0;
                        }
                    },
                    State::OpenTag => {
                        if self.buffer.len() == XML_START_PREFIX.len() && !(b.is_ascii_whitespace() || b == b'>') {
                            // Faux positif (ex: "<records") : on annule et on
                            // reprend la recherche à partir de cet octet.
                            self.buffer.clear();
                            state = State::Searching;
                            matched_start = if b == XML_START_PREFIX[0] { 1 } else { 0 };
                            continue;
                        }
                        self.buffer.push(b);
                        if b == b'>' {
                            state = State::Content;
                        }
                    },
                    State::Content => {
                        self.buffer.push(b);
                        if b == XML_END_TAG[matched_end] {
                            matched_end += 1;
                            if matched_end == XML_END_TAG.len() {
                                found = true;
                                option_record = Some(self.format.deserialize(&self.buffer)?);
                                break;
                            }
                        } else if b == XML_END_TAG[0] {
                            matched_end = 1;
                        } else {
                            matched_end = 0;
                        }
                    },
                }
            }

            // 2. Informe le BufReader qu'on a traité `consumed` octets
            self.reader.consume(consumed);
        }
        Ok(option_record)
    }
}

/// Écrit des notices en série vers un flux `W`, dans un format donné.
///
/// Pour [`Format::Marcxml`], le prologue XML et l'élément `<collection>`
/// englobant sont écrits automatiquement (ouverture au premier
/// [`Writer::write`], fermeture au [`Drop`] du `Writer`) ; pour
/// [`Format::Text`] et [`Format::Marcxml`], les notices successives sont
/// séparées par une ligne vide.
pub struct Writer<W: Write> {
    pub format: Format,
    pub writer: W,
    pub count: usize,
}

impl<W: Write> Writer<W> {

    /// Crée un writer pour `format` au-dessus du flux `writer`.
    pub fn new(format: Format, writer: W) -> Self {
        let count = 0;
        Self { format, writer, count }
    }

    /// Sérialise et écrit `record` vers le flux.
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        if self.format == Format::Marcxml && self.count == 0 {
            self.writer.write("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<collection>\n".as_bytes())?;
        }
        if self.count > 0 && (self.format == Format::Text || self.format == Format::Marcxml) {
            self.writer.write("\n".as_bytes())?;
        }
        let octets = self.format.serialize(record);
        self.writer.write(&octets)?;
        self.count += 1;
        Ok(())
    }
}

impl<W: Write> Drop for Writer<W> {
    fn drop(&mut self) {
        if self.format == Format::Marcxml {
            let _ = self.writer.write("\n</collection>\n".as_bytes());
        }
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
