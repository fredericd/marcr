use std::fmt;
use std::io::Read;
use std::io::Cursor;

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
    pub leader: [u8; 3],
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
}


pub struct Iso2709Reader<R: Read> {
    reader: R,
}

impl<R:Read> Iso2709Reader<R> {
    pub fn new(reader: R) -> Self {
        Iso2709Reader {
            reader,
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

    #[test]
    fn iso2709reader() {
        let data = b!"    ";
        let cursor = Cursor::new(data.to_vec());
        ler reader = Iso2709Reader::new(cursor);
    }

}
