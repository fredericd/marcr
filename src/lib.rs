use std::fmt;
use std::str;
use std::io::{Write, Read, BufRead};
use quick_xml::events::{BytesText, Event};
use quick_xml::writer::Writer as XmlWriter;
use quick_xml::reader::Reader as XmlReader;
use memchr::memchr;

const FT: u8 = 0x1e; // Field terminator
const RT: u8 = 0x1d; // Record terminator
const DE: u8 = 0x1f; // Delimiter
const DEFAULT_LEADER: [u8; 24] = *b"00000nam a2200000   4500";
const XML_START_TAG: &[u8] = b"<record>";
const XML_END_TAG: &[u8] = b"</record>";

#[derive(Debug)]
pub struct Subfield(pub char, pub String);

impl fmt::Display for Subfield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Subfield(letter, value) = &self;
        write!(f, "${} {}", *letter as char, value)
    }
}

/// A MARC record field representation.
///
/// A field can be a control field or a standard field
#[derive(Debug)]
pub enum Field {
    Control(u16, String),
    Standard(u16, [char; 2], Vec<Subfield>),
}

impl Field {
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

#[derive(Debug)]
pub struct Record {
    pub leader: [u8; 24],
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
    pub fn add(&mut self, field: Field) {
        let tag = field.tag();
        let pos: usize = self.fields.iter()
            .position(|item| item.tag() > tag)
            .unwrap_or(self.fields.len());
        self.fields.insert(pos, field);
    }

    pub fn new(fields: Vec<Field>) -> Self {
        let leader: [u8; 24] = DEFAULT_LEADER;
        Self {
            leader,
            fields,
        }
    }

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

#[derive(PartialEq)]
pub enum Format {
  Iso2709,
  Marcxml,
  Text,
}

#[allow(dead_code)]
pub struct RWDescription {
    pub format: Format,
    pub description: String,
    pub extension: String,
}

impl Format {
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

    pub fn deserialize(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        match self {
            Format::Iso2709 => self.deserialize_iso2709(octets),
            Format::Marcxml => self.deserialize_marcxml(octets),
            Format::Text => self.deserialize_text(octets),
        }
    }

    pub fn serialize(&self, record: &Record) -> Vec<u8> {
        match self {
            Format::Iso2709 => self.serialize_iso2709(record),
            Format::Marcxml => self.serialize_marcxml(record),
            Format::Text => self.serialize_text(record),
        }
    }

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
 
    pub fn deserialize_iso2709(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        if octets.len() < 40 { return Err("Invalid record. Too short".into()); }
        let leader: [u8; 24] = octets[..24].try_into().unwrap();
        let text = std::str::from_utf8(&octets[12..17])?;
        let directory_len: usize = text.parse::<usize>().unwrap();
        let number_of_tags = (directory_len - 24 - 1) / 12;
        let mut fields: Vec<Field> = Vec::with_capacity(number_of_tags);
        for i in 0..number_of_tags {
            let directory_offset = 24 + i * 12;
            let mut text = std::str::from_utf8(&octets[directory_offset..directory_offset+3])?;
            let tag: u16 = match text.parse::<u16>() {
                Ok(tag) => tag,
                Err(_) => return Err("Bad ISO2709, invalid tag".into()),
            };
            text = std::str::from_utf8(&octets[directory_offset+3..directory_offset+3+4])?;
            let len: usize = match text.parse::<usize>() {
                Ok(len) => len - 1,
                Err(_) => return Err("Bad ISO2709, length non digit".into()),
            };
            text = std::str::from_utf8(&octets[directory_offset+3+4..directory_offset+3+4+5])?;
            let offset: usize = text.parse::<usize>().unwrap();
            let base = directory_len + offset;
            if tag < 10 {
                let slice = &octets[base..base + len];
                let value = String::from_utf8_lossy(slice).into_owned();
                fields.push(Field::Control(tag, value));
            } else {
                let ind: [char; 2] = [octets[base] as char, octets[base+1] as char];
                let mut j = base + 2;
                let mut subfields: Vec<Subfield> = Vec::with_capacity(3);
                while j < base + len {
                    if octets[j] == DE {
                        j += 1;
                        let letter: char = octets[j] as char;
                        j += 1;
                        let mut k = j;
                        while !(octets[k] == DE || octets[k] == FT) {
                            k += 1;
                        }
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

    pub fn deserialize_text(&self, _octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        return Err("No serializer available".into());
    }


}

pub struct Reader<R> {
    pub format: Format,
    pub reader: R,
    pub buffer: Vec<u8>,   // Reusable internal buffer
    pub count: usize,
}

impl<R: Read + BufRead> Reader<R> {
    pub fn new(format: Format, reader: R) -> Self {
        let buffer: Vec<u8> = Vec::new();
        let count = 0;
        Self { format, reader, buffer, count }
    }

    pub fn read(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        match self.format {
            Format::Iso2709 => self.read_iso2709(),
            Format::Marcxml => self.read_marcxml(),
            Format::Text    => return Err("Pas de parser pour Text".into()),
        }
    }

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

    pub fn read_marcxml(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        self.buffer.clear();
        let mut found = false;
        let mut matched_start = 0;
        let mut in_record = false;
        let mut matched_end = 0;
        let mut option_record: Option<Record> = None;

        while !found {
            // 1. Accès direct au tampon mémoire du BufReader
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                break; // Fin de fichier propre
            }

            let mut consumed = 0;

            if !in_record {
                // PHASE 1 : Recherche de <record>
                for &b in available {
                    consumed += 1;
                    if b == XML_START_TAG[matched_start] {
                        matched_start += 1;
                        if matched_start == XML_START_TAG.len() {
                            in_record = true;
                            self.buffer.extend_from_slice(XML_START_TAG);
                            break;
                        }
                    } else if b == XML_START_TAG[0] {
                        matched_start = 1;
                    } else {
                        matched_start = 0;
                    }
                }
            } else {
                // PHASE 2 : Capture du contenu jusqu'à </record>
                for &b in available {
                    consumed += 1;
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
                }
            }

            // 2. Informe le BufReader qu'on a traité `consumed` octets
            self.reader.consume(consumed);
        }
        Ok(option_record)
    }
}

pub struct Writer<W: Write> {
    pub format: Format,
    pub writer: W,
    pub count: usize,
}

impl<W: Write> Writer<W> {

    pub fn new(format: Format, writer: W) -> Self {
        let count = 0;
        Self { format, writer, count }
    }

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
