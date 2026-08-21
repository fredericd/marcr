use std::fmt;
use std::str;
use std::io::{ Write, Read, BufRead, Split };
use quick_xml::events::{BytesDecl, BytesText, BytesStart, BytesEnd, Event};
use quick_xml::writer::Writer as XmlWriter;


const FT: u8 = 0x1e; // Field terminator
const RT: u8 = 0x1d; // Record terminator
const DE: u8 = 0x1f; // Delimiter
const DEFAULT_LEADER: [u8; 24] = *b"00000nam a2200000   4500";

#[derive(Debug)]
pub struct Subfield(pub char, pub String);

impl fmt::Display for Subfield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Subfield(letter, value) = &self;
        write!(f, "${} {}", *letter as char, value)
    }
}

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

    pub fn new_empty() -> Self {
        let leader: [u8; 24] = DEFAULT_LEADER;
        let fields: Vec<Field> = Vec::new();
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
    pub fn get_available_readers() -> Vec<RWDescription> {
        vec![
            RWDescription{
                format: Format::Iso2709,
                description: String::from("ISO 2709"),
                extension: String::from("mrc"),
            },
        ]
    }

    pub fn get_available_writers() -> Vec<RWDescription> {
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
}



pub trait RecordWriter {
    /// Écrit un enregistrement MARC dans le flux
    fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>>;

    /// Vide les tampons sous-jacents si nécessaire
    fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Implémentation par défaut vide
    }
}


pub trait RecordReader {
    /// Écrit un enregistrement structuré dans le flux
    fn read(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>>;

    /// Vide les tampons sous-jacents si nécessaire
    fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(()) // Implémentation par défaut vide
    }
}


pub enum Reader<R: Read + BufRead> {
    Iso2709(Iso2709Reader<R>),
}

impl<R: Read + BufRead> Reader<R> {
    /// Constructeur basé sur l'enum Format
    pub fn new(format: Format, reader: R) -> Self {
        match format {
            Format::Iso2709 => Reader::Iso2709(Iso2709Reader::new(reader)),
            Format::Marcxml => panic!("Pas Marxml"),
            Format::Text => panic!("Pas Text Reader"),
        }
    }
    
    /// Méthode d'interface unifiée
    pub fn read(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        match self {
            Reader::Iso2709(r) => r.read(),
        }
    }
}

pub enum Writer<W: Write> {
    Iso2709(Iso2709Writer<W>),
    Marcxml(MarcxmlWriter<W>),
    Text(TextWriter<W>),
}

impl<W: Write> Writer<W> {
    /// Constructeur basé sur l'enum Format
    pub fn new(format: Format, writer: W) -> Self {
        match format {
            Format::Iso2709 => Writer::Iso2709(Iso2709Writer::new(writer)),
            Format::Marcxml => Writer::Marcxml(MarcxmlWriter::new(writer)),
            Format::Text => Writer::Text(TextWriter::new(writer)),
        }
    }
}
    
impl<W: Write> RecordWriter for Writer<W> {
    /// Méthode d'interface unifiée
    fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Writer::Iso2709(w) => w.write(record),
            Writer::Marcxml(w) => w.write(record),
            Writer::Text(w) => w.write(record),
        }
    }
}

pub struct Iso2709Writer<W: Write> {
    writer: W,
    pub count: usize,
}

impl<W:Write> Iso2709Writer<W> {
    /// Create a new MarcxmlWriter
    ///
    /// # Arguments
    ///
    /// - `writer` - Any target implementing the trait [`std::io::Write`]
    pub fn new(writer: W) -> Self {
        Iso2709Writer {
            writer,
            count: 0,
        }
    }

    /// Write a MARC record
    ///
    /// # Arguments
    ///
    /// - `record`
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        let mut fields: Vec<u8> = Vec::new();
        let mut directory: Vec<u8> = Vec::new();
        let mut from = 0;
        for field in record.fields.iter() {
            let mut data: Vec<u8> = Vec::new();
            let tag = match field {
                Field::Control(tag, value) => {
                    data.extend_from_slice(value.as_bytes());
                    tag
                },
                Field::Standard(tag, ind, subfields) => {
                    data.push(ind[0] as u8);
                    data.push(ind[1] as u8);
                    for Subfield(letter, value) in subfields {
                        data.push(DE);
                        data.push(*letter as u8);
                        data.extend_from_slice(value.as_bytes());
                    }
                    tag
                }
            };
            data.push(FT);
            let len = data.len();
            directory.extend_from_slice(format!("{tag:03}").as_bytes());
            directory.extend_from_slice(format!("{len:04}").as_bytes());
            directory.extend_from_slice(format!("{from:05}").as_bytes());
            from = from + len;
            fields.append(&mut data);
        }
        let offset = 24 + 12 * record.fields.len() + 1;
        let length = offset + from + 1;
        let mut leader = record.leader;
        leader[..5].copy_from_slice(format!("{length:05}").as_bytes());
        leader[12..17].copy_from_slice(format!("{offset:05}").as_bytes());
        directory.push(FT);
        fields.push(RT);
        let mut data: Vec<u8> = leader.to_vec();
        data.extend_from_slice(&directory);
        data.extend_from_slice(&fields);
        self.writer.write(&data)?;
        self.count = self.count + 1;
        Ok(())
    }
}

pub struct Iso2709Reader<R: Read> {
    iterator: Split<R>,
    pub count: usize,
}

impl<R:Read + BufRead> Iso2709Reader<R> {
    /// Create a new Iso2709Reader
    ///
    /// # Arguments
    ///
    /// - `reader` - Any target implementing the trait [`std::io::Read`]
    pub fn new(reader: R) -> Self {
        let iterator = reader.split(RT);
        Iso2709Reader {
            iterator,
            count: 0,
        }
    }
    
    /// Read a MARC record
    ///
    /// # Arguments
    ///
    /// - `record`
    pub fn read(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        let result = match self.iterator.next() {
            Some(raw) => raw?,
            None      => { return Ok(None); } // No more data to read
        };
        let raw: &[u8] = &result;
        //if raw.len() < 40 { return Box(Err("Invalid record. Too short")); }
        let leader: [u8; 24] = raw[..24].try_into().unwrap();
        let text = std::str::from_utf8(&raw[12..17])?;
        let directory_len: usize = text.parse::<usize>().unwrap();
        let number_of_tags = (directory_len - 24 - 1) / 12;
        let mut fields: Vec<Field> = Vec::with_capacity(number_of_tags);
        for i in 0..number_of_tags {
            let directory_offset = 24 + i * 12;
            let mut text = std::str::from_utf8(&raw[directory_offset..directory_offset+3])?;
            let tag: u16 = text.parse::<u16>().unwrap();
            text = std::str::from_utf8(&raw[directory_offset+3..directory_offset+3+4])?;
            let len: usize = text.parse::<usize>().unwrap() - 1;
            text = std::str::from_utf8(&raw[directory_offset+3+4..directory_offset+3+4+5])?;
            let offset: usize = text.parse::<usize>().unwrap();
            let base = directory_len + offset;
            if tag < 10 {
                let octets = &raw[base..base + len];
                let value: String = std::str::from_utf8(octets)?.to_string();
                fields.push(Field::Control(tag, value));
            } else {
                let ind: [char; 2] = [raw[base] as char, raw[base+1] as char];
                let mut j = base + 2;
                let mut subfields: Vec<Subfield> = Vec::new();
                while j < base + len {
                    if raw[j] == DE {
                        j += 1;
                        let letter: char = raw[j] as char;
                        j += 1;
                        let mut k = j;
                        while !(raw[k] == DE || raw[k] == FT) {
                            k += 1;
                        }
                        let octets = &raw[j..k];
                        let value: String = std::str::from_utf8(octets)?.to_string();
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
        self.count = self.count + 1;
        Ok(Some(record))
    }
}

pub struct TextWriter<W: Write> {
    writer: W,
    count: usize,
}

impl<W:Write> TextWriter<W> {
    /// Create a new TextWriter
    ///
    /// # Arguments
    ///
    /// - `writer` - Any target implementing the trait [`std::io::Write`]
    pub fn new(writer: W) -> Self {
        TextWriter {
            writer,
            count: 0,
        }
    }

    /// Write a MARC record
    ///
    /// # Arguments
    ///
    /// - `record`
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        self.count = self.count + 1;
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
        self.writer.write(lines.join("\n").as_bytes())?;
        Ok(())
    }
}

pub struct MarcxmlWriter<W: Write> {
    writer: XmlWriter<W>,
    pub count: usize,
}

impl<W:Write> MarcxmlWriter<W> {
    /// Create a new MarcxmlWriter
    ///
    /// # Arguments
    ///
    /// - `writer` - Any target implementing the trait [`std::io::Write`]
    pub fn new(writer: W) -> Self {
        let xml_writer = XmlWriter::new_with_indent(writer, b' ', 2);
        MarcxmlWriter {
            writer: xml_writer,
            count: 0,
        }
    }
    
    /// Write a MARC record
    ///
    /// # Arguments
    ///
    /// - `record`
    pub fn write(&mut self, record: &Record) -> Result<(), Box<dyn std::error::Error>> {
        if self.count == 0 {
            self.writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;
            self.writer.write_event(Event::Start(BytesStart::new("collection")))?;
        }

        self.writer
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
            })?;
        self.count = self.count + 1;
        Ok(())
    }
}

impl<W: Write> Drop for MarcxmlWriter<W> {
    fn drop(&mut self) {
        let event = Event::End(BytesEnd::new("collection"));
        if let Err(e) = self.writer.write_event(event) {
            panic!("drop MarcxmlWriter: {e}");
        }
    }
}


//
// Module des tests unitaires
//
#[cfg(test)]
mod tests {
    use super::*;

    fn get_default_record() -> Record {
        let leader: [u8; 24] = DEFAULT_LEADER;
        let fields = vec![
            Field::Control(001, String::from("000001")),
            Field::Control(005, String::from("2026")),
            Field::Standard(200, [' ', '1'],
                vec![
                    Subfield('a', String::from("Mon titre")),
                    Subfield('e', String::from("Complément du titre")),
                ]),
            Field::Standard(700, [' ', '1'],
                vec![
                    Subfield('a', "Demians".to_string()),
                    Subfield('b', String::from("Frédéric")),
                ]),
        ];
        Record {
            leader,
            fields
        }
    }

    #[test]
    fn field_control_display() {
        let field = Field::Control(001, "1234".to_string());
        let text = field.to_string();
        assert_eq!(text, "001    1234");
    }

    #[test]
    fn field_standard_display() {
        let field = Field::Standard(200, ['0', ' '], vec![
            Subfield('a', "Mon titre".to_string()),
            Subfield('e', "Mon sous-titre".to_string()),
        ]);
        let text = field.to_string();
        assert_eq!(text, "200 0  $a Mon titre $e Mon sous-titre");
    }

    #[test]
    fn record_display() {
        let record = get_default_record();
        let text = record.to_string();
        let expected = String::from("00000nam a2200000   4500
001    000001
005    2026
200  1 $a Mon titre $e Complément du titre
700  1 $a Demians $b Frédéric");
        assert_eq!(text, expected);
    }

    #[test]
    fn rw_formater() -> Result<(), Box<dyn std::error::Error>> {
        let writers = Format::get_available_writers();
        for writer in writers {
            println!("{} / {}", writer.description, writer.extension);
        }
        Ok(())
    }

    #[test]
    fn text_writer() -> Result<(), Box<dyn std::error::Error>> {
        let record = get_default_record();
        let mut writer = TextWriter::new(std::io::Cursor::new(Vec::new()));
        let _ = writer.write(&record);
        let octets: Vec<u8> = writer.writer.into_inner();
        let text = String::from_utf8(octets)?;
        let expected = String::from("00000nam a2200000   4500
001    000001
005    2026
200  1 $a Mon titre $e Complément du titre
700  1 $a Demians $b Frédéric

");
        assert_eq!(text, expected);
        Ok(())
    }

    #[test]
    fn iso2709_writer() -> Result<(), Box<dyn std::error::Error>> {
        let record = get_default_record();
        let mut writer = Iso2709Writer::new(std::io::Cursor::new(Vec::new()));
        let _ = writer.write(&record);
        let octets: Vec<u8> = writer.writer.into_inner();
        let text = String::from_utf8(octets)?;
        let expected = String::from("00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}");
        assert_eq!(text, expected);
        Ok(())
    }

    #[test]
    fn iso2709_reader() {
        let raw = String::from("00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}");
        let cursor = std::io::Cursor::new(raw.as_bytes());
        let mut reader = Iso2709Reader::new(cursor);
        match reader.read() {
            Ok(value) => {
                match value {
                    Some(record) => {
                        let text = record.to_string();
                        let expected = String::from("00146nam a2200073   4500
001    000001
005    2026
200  1 $a Mon titre $e Complément du titre
700  1 $a Demians $b Frédéric");
                        assert_eq!(text, expected);

                    },
                    None => panic!("End of file")
                }
            },
            Err(e) => {
                panic!("Reading error: {e}");
            }
        }
    }

    #[test]
    fn record_insert() {
        let mut record = Record::new_empty();
        record.insert(vec![
            vec!["200", "  ", "a", "Mon titre", "b", "Texte imprimé", "e", "Complément"],
            vec!["010", "  ", "a", "9782070368228"],
            vec!["610", "  ", "a", "Sujet 1"],
            vec!["610", "  ", "a", "Sujet 2"],
            vec!["600", "  ", "a", "Personnage 1"],
            vec!["600", "  ", "a", "Personnage 2"],
            vec!["001", "PPN1234"],
        ]);
        let text = record.to_string();
        let expected = String::from("00000nam a2200000   4500
001    PPN1234
010    $a 9782070368228
200    $a Mon titre $b Texte imprimé $e Complément
600    $a Personnage 1
600    $a Personnage 2
610    $a Sujet 1
610    $a Sujet 2");
        assert_eq!(text, expected);
    }

}
