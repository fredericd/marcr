use std::str;

use quick_xml::escape::unescape;
use quick_xml::events::{BytesText, Event};
use quick_xml::reader::Reader as XmlReader;
use quick_xml::writer::Writer as XmlWriter;
use quick_xml::XmlVersion;

use crate::{Field, Record, Subfield};

use super::Format;

impl Format {
    /// Serializes `record` to a MARCXML `<record>` element (indented),
    /// with no XML prologue nor enclosing `<collection>` element — see
    /// [`crate::Writer`] to produce a complete MARCXML document.
    pub fn serialize_marcxml(&self, record: &Record) -> Vec<u8> {
        let cursor = std::io::Cursor::new(Vec::new());
        let mut xml_writer = XmlWriter::new_with_indent(cursor, b' ', 2);
        match xml_writer
            .create_element("record")
            .write_inner_content(|writer| {
                let leader_str = String::from_utf8_lossy(&record.leader);
                writer
                    .create_element("leader")
                    .write_text_content(BytesText::new(&leader_str))?;
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

    /// Parses a MARCXML `<record>` element (attributes and namespace
    /// ignored; only the `leader`, `controlfield`, `datafield` and
    /// `subfield` elements are recognized). `octets` must be valid UTF-8.
    pub fn deserialize_marcxml(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        let xml: &str = std::str::from_utf8(octets)?;
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
                    let contenu = unescape(&reader.read_text(e.name())?.decode()?)?.into_owned();
                    let leader = contenu.as_bytes();
                    if leader.len() == 24 {
                        record.leader[..24].copy_from_slice(&leader[..24]);
                    }
                },
                Event::Start(e) if e.name().as_ref() == b"controlfield" => {
                    let tag = e.attributes()
                        .flatten()
                        .find(|attr| attr.key.as_ref() == b"tag")
                        .map(|attr| attr.normalized_value(XmlVersion::Implicit1_0).map(|v| v.into_owned()))
                        .transpose()?;
                    if let Some(t) = tag {
                        let contenu = unescape(&reader.read_text(e.name())?.decode()?)?.into_owned();
                        let tt: u16 = t.parse().map_err(|_| "Bad MARCXML, invalid controlfield tag")?;
                        let cf = Field::Control(tt, contenu);
                        record.fields.push(cf);
                    }
                },
                Event::Start(e) if e.name().as_ref() == b"datafield" => {
                    // Extract: tag, ind1, ind2
                    let mut tag: Option<u16> = None;
                    let mut ind1 = None;
                    let mut ind2 = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"tag" => tag = Some(attr.normalized_value(XmlVersion::Implicit1_0)?.parse()
                                .map_err(|_| "Bad MARCXML, invalid datafield tag")?),
                            b"ind1" => ind1 = Some(attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned()),
                            b"ind2" => ind2 = Some(attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned()),
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
                    // Extract: code
                    let mut code = None;
                    for attr in e.attributes().flatten() {
                        match attr.key.as_ref() {
                            b"code" => code = Some(attr.normalized_value(XmlVersion::Implicit1_0)?.into_owned()),
                            _ => (),
                        };
                    }
                    if let Some(code) = code {
                        let contenu = unescape(&reader.read_text(e.name())?.decode()?)?.into_owned();
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
}
