use super::*;

const EXPECTED_DEFAULT_RECORD: &str = "00146nam a2200073   4500
001 000001
005 2026
200  1 $a Mon titre $e Complément du titre
700  1 $a Demians $b Frédéric";

fn get_default_record() -> Record {
    let mut leader: [u8; 24] = DEFAULT_LEADER;
    leader[0..5].copy_from_slice(b"00146");
    leader[15..17].copy_from_slice(b"73");
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
    assert_eq!(text, "001 1234");
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
    let expected = String::from(EXPECTED_DEFAULT_RECORD);
    assert_eq!(text, expected);
}

#[test]
fn text_writer() -> Result<(), Box<dyn std::error::Error>> {
    let record = get_default_record();
    let format = Format::Text;
    let mut writer = Writer::new(format, std::io::Cursor::new(Vec::new()));
    let _ = writer.write(&record);
    let octets: Vec<u8> = writer.writer.get_ref().as_slice().to_vec();
    let text = String::from_utf8(octets)?;
    let expected = String::from(EXPECTED_DEFAULT_RECORD) + "\n";
    assert_eq!(text, expected);
    Ok(())
}

#[test]
fn text_reader() {
    let record = Format::Text.deserialize(EXPECTED_DEFAULT_RECORD.as_bytes())
        .expect("Text deserialization failed");
    assert_eq!(record.to_string(), EXPECTED_DEFAULT_RECORD);
}

#[test]
fn text_reader_control_field_single_space_separator() {
    // Reported case: a control field with a very short value, e.g.
    // "001 2", must not be rejected by a minimum length meant for
    // standard fields (which have indicators).
    let text = "00000nam a2200000   4500\n001 2";
    let record = Format::Text.deserialize(text.as_bytes())
        .expect("Text deserialization failed");
    match &record.fields[0] {
        Field::Control(tag, value) => {
            assert_eq!(*tag, 1);
            assert_eq!(value, "2");
        },
        other => panic!("Expected a control field, got {other:?}"),
    }
}

#[test]
fn text_roundtrip() {
    // serialize_text() then deserialize_text() should yield the same record.
    let record = get_default_record();
    let octets = Format::Text.serialize(&record);
    let parsed = Format::Text.deserialize(&octets)
        .expect("Text deserialization failed");
    assert_eq!(parsed.to_string(), record.to_string());
}

#[test]
fn text_reader_multi_record_stream() {
    // Writer separates text records with a blank line; Reader must
    // retrieve exactly the same records in reverse.
    let record = get_default_record();
    let mut writer = Writer::new(Format::Text, std::io::Cursor::new(Vec::new()));
    writer.write(&record).unwrap();
    writer.write(&record).unwrap();
    let octets: Vec<u8> = writer.writer.get_ref().as_slice().to_vec();
    drop(writer);

    let mut reader = Reader::new(Format::Text, std::io::Cursor::new(octets));
    let expected = String::from(EXPECTED_DEFAULT_RECORD);
    for _ in 0..2 {
        match reader.read() {
            Ok(Some(record)) => assert_eq!(record.to_string(), expected),
            Ok(None) => panic!("Premature end of stream"),
            Err(e) => panic!("Read error: {e}"),
        }
    }
    match reader.read() {
        Ok(None) => (),
        other => panic!("Expected None at end of stream, got {other:?}"),
    }
}

#[test]
fn text_reader_empty_subfields() {
    // A standard field with no subfield must remain valid (indicators
    // kept, empty subfield list): serializing then deserializing avoids
    // hand-writing a fragile string full of spaces.
    let record = Record::new(vec![Field::Standard(200, [' ', ' '], vec![])]);
    let octets = Format::Text.serialize(&record);
    let parsed = Format::Text.deserialize(&octets)
        .expect("Text deserialization failed");
    match &parsed.fields[0] {
        Field::Standard(tag, ind, subfields) => {
            assert_eq!(*tag, 200);
            assert_eq!(*ind, [' ', ' ']);
            assert!(subfields.is_empty());
        },
        other => panic!("Expected a standard field, got {other:?}"),
    }
}

#[test]
fn iso2709_writer() -> Result<(), Box<dyn std::error::Error>> {
    let record = get_default_record();
    let mut writer = Writer::new(Format::Iso2709, std::io::Cursor::new(Vec::new()));
    let _ = writer.write(&record);
    let octets: Vec<u8> = writer.writer.get_ref().as_slice().to_vec();
    let text = String::from_utf8(octets)?;
    let expected = String::from("00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}");
    assert_eq!(text, expected);
    Ok(())
}

#[test]
fn iso2709_reader() {
    let raw = String::from("00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}");
    let cursor = std::io::Cursor::new(raw.as_bytes());
    let mut reader = Reader::new(Format::Iso2709, cursor);
    match reader.read() {
        Ok(value) => {
            match value {
                Some(record) => {
                    let text = record.to_string();
                    let expected = String::from(EXPECTED_DEFAULT_RECORD);
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
fn marcxml_deserialize() {
    let xml = String::from("<record>
  <leader>00146nam a2200073   4500</leader>
  <controlfield tag=\"001\">000001</controlfield>
  <controlfield tag=\"005\">2026</controlfield>
  <datafield tag=\"200\" ind1=\" \" ind2=\"1\">
    <subfield code=\"a\">Mon titre</subfield>
    <subfield code=\"e\">Complément du titre</subfield>
  </datafield>
  <datafield tag=\"700\" ind1=\" \" ind2=\"1\">
    <subfield code=\"a\">Demians</subfield>
    <subfield code=\"b\">Frédéric</subfield>
  </datafield>
</record>");
    let format = Format::Marcxml;
    match format.deserialize(xml.as_bytes()) {
        Ok(record) => {
            let expected = String::from(EXPECTED_DEFAULT_RECORD);
            assert_eq!(record.to_string(), expected);
        },
        Err(e) => println!("{e}"),
    };
}

#[test]
fn marcxml_reader_accepts_start_tag_with_attributes() {
    // <record> may carry attributes (xmlns, ...) in real-world exports;
    // the scanner must not require the exact literal "<record>".
    let record_body = "
  <leader>00146nam a2200073   4500</leader>
  <controlfield tag=\"001\">000001</controlfield>
  <controlfield tag=\"005\">2026</controlfield>
  <datafield tag=\"200\" ind1=\" \" ind2=\"1\">
    <subfield code=\"a\">Mon titre</subfield>
    <subfield code=\"e\">Complément du titre</subfield>
  </datafield>
  <datafield tag=\"700\" ind1=\" \" ind2=\"1\">
    <subfield code=\"a\">Demians</subfield>
    <subfield code=\"b\">Frédéric</subfield>
  </datafield>
</record>";
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <collection xmlns=\"http://www.loc.gov/MARC21/slim\">\n\
         <record xmlns=\"http://www.loc.gov/MARC21/slim\">{record_body}\n\
         <record   >{record_body}\n\
         </collection>\n"
    );
    let cursor = std::io::Cursor::new(xml.into_bytes());
    let mut reader = Reader::new(Format::Marcxml, cursor);
    let expected = String::from(EXPECTED_DEFAULT_RECORD);

    for _ in 0..2 {
        match reader.read() {
            Ok(Some(record)) => assert_eq!(record.to_string(), expected),
            Ok(None) => panic!("Premature end of stream"),
            Err(e) => panic!("Read error: {e}"),
        }
    }
}

#[test]
fn marcxml_reader_ignores_lookalike_tag() {
    // <records> must not be confused with <record>.
    let xml = "<records><record>
  <leader>00146nam a2200073   4500</leader>
  <controlfield tag=\"001\">000001</controlfield>
  <controlfield tag=\"005\">2026</controlfield>
  <datafield tag=\"200\" ind1=\" \" ind2=\"1\">
    <subfield code=\"a\">Mon titre</subfield>
    <subfield code=\"e\">Complément du titre</subfield>
  </datafield>
  <datafield tag=\"700\" ind1=\" \" ind2=\"1\">
    <subfield code=\"a\">Demians</subfield>
    <subfield code=\"b\">Frédéric</subfield>
  </datafield>
</record></records>";
    let cursor = std::io::Cursor::new(xml.as_bytes().to_vec());
    let mut reader = Reader::new(Format::Marcxml, cursor);
    let expected = String::from(EXPECTED_DEFAULT_RECORD);
    match reader.read() {
        Ok(Some(record)) => assert_eq!(record.to_string(), expected),
        Ok(None) => panic!("Premature end of stream"),
        Err(e) => panic!("Read error: {e}"),
    }
}

#[test]
fn record_insert() {
    let mut record = Record::default();
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
001 PPN1234
010    $a 9782070368228
200    $a Mon titre $b Texte imprimé $e Complément
600    $a Personnage 1
600    $a Personnage 2
610    $a Sujet 1
610    $a Sujet 2");
    assert_eq!(text, expected);
}

#[test]
fn record_fields_by_tag() {
    let mut record = Record::default();
    record.insert(vec![
        vec!["610", "  ", "a", "Sujet 1"],
        vec!["610", "  ", "a", "Sujet 2"],
        vec!["600", "  ", "a", "Personnage 1"],
        vec!["001", "PPN1234"],
    ]);

    let values: Vec<&str> = record.fields_by_tag(610)
        .map(|field| match field {
            Field::Standard(_, _, subfields) => subfields[0].1.as_str(),
            _ => panic!("Expected a standard field"),
        })
        .collect();
    assert_eq!(values, vec!["Sujet 1", "Sujet 2"]);

    assert_eq!(record.fields_by_tag(999).count(), 0);
}

#[test]
fn field_subfield() {
    let mut record = Record::default();
    record.insert(vec![
        vec!["001", "PPN1234"],
        vec!["200", " 1", "a", "Titre 1", "e", "Complément", "a", "Titre 2"],
    ]);
    let field = record.field(200).unwrap();

    // The first occurrence of a repeated code is returned.
    assert_eq!(field.subfield('a'), Some("Titre 1"));
    assert_eq!(field.subfield('e'), Some("Complément"));
    assert_eq!(field.subfield('z'), None);
    // A control field has no subfields.
    assert_eq!(record.field(1).unwrap().subfield('a'), None);
}

#[test]
fn record_field() {
    let mut record = Record::default();
    record.insert(vec![
        vec!["001", "PPN1234"],
        vec!["610", "  ", "a", "Sujet 1"],
        vec!["610", "  ", "a", "Sujet 2"],
    ]);

    // The first occurrence is returned, not the last one.
    match record.field(610) {
        Some(Field::Standard(_, _, subfields)) => assert_eq!(subfields[0].1, "Sujet 1"),
        other => panic!("Expected a standard field, got {other:?}"),
    }
    match record.field(1) {
        Some(Field::Control(_, value)) => assert_eq!(value, "PPN1234"),
        other => panic!("Expected a control field, got {other:?}"),
    }
    assert!(record.field(999).is_none());
}

#[test]
fn record_remove_tag() {
    let mut record = Record::default();
    record.insert(vec![
        vec!["610", "  ", "a", "Sujet 1"],
        vec!["610", "  ", "a", "Sujet 2"],
        vec!["600", "  ", "a", "Personnage 1"],
        vec!["001", "PPN1234"],
    ]);

    let removed = record.remove_tag(610);
    assert_eq!(removed.len(), 2);
    assert!(removed.iter().all(|f| *f.tag() == 610));
    assert_eq!(record.fields_by_tag(610).count(), 0);
    // Remaining fields keep their relative order.
    let remaining_tags: Vec<u16> = record.fields.iter().map(|f| *f.tag()).collect();
    assert_eq!(remaining_tags, vec![1, 600]);

    assert!(record.remove_tag(999).is_empty());
}

#[test]
fn record_insert_single_char_value_and_short_indicators() {
    let mut record = Record::default();
    record.insert(vec![
        // A one-character value must be kept.
        vec!["995", "  ", "r", "X"],
        // Fewer than two indicators: entry ignored, no panic.
        vec!["200", "1", "a", "Titre"],
    ]);
    assert_eq!(record.fields.len(), 1);
    assert_eq!(record.field(995).unwrap().subfield('r'), Some("X"));
}

#[test]
fn record_field_mut() {
    let mut record = Record::default();
    record.insert(vec![
        vec!["001", "PPN1234"],
        vec!["610", "  ", "a", "Sujet 1"],
        vec!["610", "  ", "a", "Sujet 2"],
    ]);

    // Only the first occurrence is modified.
    if let Some(Field::Standard(_, ind, _)) = record.field_mut(610) {
        ind[0] = '1';
    }
    let indicators: Vec<char> = record.fields_by_tag(610)
        .map(|field| match field {
            Field::Standard(_, ind, _) => ind[0],
            _ => panic!("Expected a standard field"),
        })
        .collect();
    assert_eq!(indicators, vec!['1', ' ']);

    if let Some(Field::Control(_, value)) = record.field_mut(1) {
        value.push_str("X");
    }
    assert!(matches!(record.field(1), Some(Field::Control(_, value)) if value == "PPN1234X"));
    assert!(record.field_mut(999).is_none());
}

#[test]
fn record_fields_by_tag_mut() {
    let mut record = Record::default();
    record.insert(vec![
        vec!["610", "  ", "a", "Sujet 1"],
        vec!["600", "  ", "a", "Personnage 1"],
        vec!["610", "  ", "a", "Sujet 2"],
    ]);

    for field in record.fields_by_tag_mut(610) {
        if let Field::Standard(_, _, subfields) = field {
            subfields.push(Subfield('2', String::from("rameau")));
        }
    }
    assert!(record.fields_by_tag(610).all(|field| field.subfield('2') == Some("rameau")));
    assert_eq!(record.field(600).unwrap().subfield('2'), None);
}

#[test]
fn iso2709_reader_continues_after_malformed_record() {
    // A malformed record must yield an error without blocking the stream:
    // the next call to read() returns the following record.
    let good = "00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}";
    let bad = good.replacen("4500001", "45000X1", 1); // invalid tag in the directory
    let raw = format!("{good}{bad}{good}");
    let mut reader = Reader::new(Format::Iso2709, std::io::Cursor::new(raw.into_bytes()));
    let expected = String::from(EXPECTED_DEFAULT_RECORD);

    match reader.read() {
        Ok(Some(record)) => assert_eq!(record.to_string(), expected),
        other => panic!("Expected first record, got {other:?}"),
    }
    assert_eq!(reader.count, 1);
    assert!(reader.read().is_err(), "Expected an error on the malformed record");
    assert_eq!(reader.count, 2, "A malformed record must be counted");
    match reader.read() {
        Ok(Some(record)) => assert_eq!(record.to_string(), expected),
        other => panic!("Expected third record, got {other:?}"),
    }
    match reader.read() {
        Ok(None) => (),
        other => panic!("Expected None at end of stream, got {other:?}"),
    }
    assert_eq!(reader.count, 3, "End of stream must not be counted");
}

const DEFAULT_ISO2709: &str = "00146nam a2200073   4500001000700000005000500007200003600012700002400048\u{1e}000001\u{1e}2026\u{1e} 1\u{1f}aMon titre\u{1f}eComplément du titre\u{1e} 1\u{1f}aDemians\u{1f}bFrédéric\u{1e}\u{1d}";

#[test]
fn iso2709_malformed_records_return_errors() {
    // Each corruption must yield an Err, never a panic.
    let corrupt = |from: &str, to: &str| -> Vec<u8> {
        assert!(DEFAULT_ISO2709.contains(from));
        DEFAULT_ISO2709.replacen(from, to, 1).into_bytes()
    };
    let mut non_ascii_leader = DEFAULT_ISO2709.as_bytes().to_vec();
    non_ascii_leader[5] = 0xff;
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("base address before the directory end", corrupt("a2200073", "a2200010")),
        ("base address beyond the record", corrupt("a2200073", "a2299999")),
        ("field offset beyond the record", corrupt("700002400048", "700002409999")),
        ("field length beyond the record", corrupt("700002400048", "700999900048")),
        ("zero field length", corrupt("001000700000", "001000000000")),
        ("missing indicators", corrupt("200003600012", "200000100012")),
        ("delimiter as last byte of a field", corrupt("200003600012", "200001500012")),
        ("non-ASCII leader", non_ascii_leader),
    ];
    for (name, octets) in cases {
        assert!(Format::Iso2709.deserialize(&octets).is_err(), "Expected an error: {name}");
    }
    // The untouched record still parses.
    assert!(Format::Iso2709.deserialize(DEFAULT_ISO2709.as_bytes()).is_ok());
}

#[test]
fn marcxml_malformed_records_return_errors() {
    let invalid_controlfield_tag = "<record><controlfield tag=\"xyz\">1</controlfield></record>";
    let invalid_datafield_tag = "<record><datafield tag=\"xyz\" ind1=\" \" ind2=\" \"></datafield></record>";
    let mut invalid_utf8 = b"<record><controlfield tag=\"001\">".to_vec();
    invalid_utf8.extend_from_slice(&[0xff, 0xfe]);
    invalid_utf8.extend_from_slice(b"</controlfield></record>");

    assert!(Format::Marcxml.deserialize(invalid_controlfield_tag.as_bytes()).is_err());
    assert!(Format::Marcxml.deserialize(invalid_datafield_tag.as_bytes()).is_err());
    assert!(Format::Marcxml.deserialize(&invalid_utf8).is_err());
}
