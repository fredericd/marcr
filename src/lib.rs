use std::fmt;
use std::io::{ Write };
use quick_xml::events::{BytesDecl, BytesText, BytesStart, BytesEnd, Event};
use quick_xml::writer::Writer as XmlWriter;

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
    pub leader: [u8; 23],
    pub fields: Vec<Field>,
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = self.fields
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        write!(f, "{text}")
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
        let leader: [u8; 23] = *b"02761nam a2200445   450";
        Self {
            leader,
            fields,
        }
    }

    pub fn insert(a_a_a: Vec<Vec<&str>>) {
        for a_a in a_a_a {
            let len = a_a.len();
            if len < 2 { continue; }
            let tag_str = a_a[0];
            if let Ok(tag) = tag_str.parse::<u16>() {
                if tag <= 9 {
                    if len == 2 {
                        let field = Field::Control(tag, String::from(a_a[1]));
                    }
                }
                else if tag <= 999 {

                }
            }
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

        self.count = self.count + 1;
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

#[cfg(test)]
mod marc_record {
    use super::*;

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
        let leader = [1,1,1];
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
        let record = Record {
            leader,
            fields
        };
        let text = record.to_string();
        let expected = String::from("001    000001
005    2026
200  1 $a Mon titre $e Complément du titre
700  1 $a Demians $b Frédéric");
        assert_eq!(text, expected);
    }

}
