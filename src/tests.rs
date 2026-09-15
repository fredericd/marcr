use super::*;

const EXPECTED_DEFAULT_RECORD: &str = "00146nam a2200073   4500
001    000001
005    2026
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
    // <record> peut porter des attributs (xmlns, ...) dans des exports
    // réels ; le scanner ne doit pas exiger le littéral exact "<record>".
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
            Ok(None) => panic!("Fin de flux prématurée"),
            Err(e) => panic!("Erreur de lecture: {e}"),
        }
    }
}

#[test]
fn marcxml_reader_ignores_lookalike_tag() {
    // <records> ne doit pas être confondu avec <record>.
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
        Ok(None) => panic!("Fin de flux prématurée"),
        Err(e) => panic!("Erreur de lecture: {e}"),
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
001    PPN1234
010    $a 9782070368228
200    $a Mon titre $b Texte imprimé $e Complément
600    $a Personnage 1
600    $a Personnage 2
610    $a Sujet 1
610    $a Sujet 2");
    assert_eq!(text, expected);
}
