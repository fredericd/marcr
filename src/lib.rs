use std::fmt;


#[derive(Debug)]
pub struct Tag(u16);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.0;
        write!(f, "{}", format!("{value:03}"))
    }
}

pub struct Subfield {
    pub letter: char,
    pub value: String,
}

impl fmt::Display for Subfield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = &self.letter;
        let value = &self.value;
        write!(f, "${} {}", *letter as char, value)
    }
}

pub enum Field {
    Control {
        tag: Tag,
        value: String,
    },
    Standard {
        tag: Tag,
        ind: [char; 2],
        subfs: Vec<Subfield>,
    },
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::Control { tag, value } => write!(f, "{tag}    {value}"),
            Field::Standard { tag, ind, subfs } => {
                let ind1 = ind[0];
                let ind2 = ind[1];
                let concatenated = subfs
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                write!(f, "{tag} {ind1}{ind2} {concatenated}")
            },
        }
    }
}

pub struct Record {
    pub leader: [u8; 3],
    pub fields: Vec<Field>,
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for field in &self.fields {
            writeln!(f, "{field}")?;
        }
        Ok(())
    }
}

impl Record {
    pub fn new() -> Self {
        let leader = [1,1,1];
        let fields = vec![
            Field::Control { tag: Tag(001), value: String::from("000001") },
            Field::Control { tag: Tag(005), value: String::from("2026") },
            Field::Standard {
                tag: Tag(200),
                ind: [' ', '1'],
                subfs: vec![
                    Subfield { letter: 'a', value: String::from("Mon titre") },
                    Subfield { letter: 'e', value: String::from("Complément du titre") },
                ],
            },
            Field::Standard {
                tag: Tag(700),
                ind: [' ', '1'],
                subfs: vec![
                    Subfield { letter: 'a', value: String::from("Demians") },
                    Subfield { letter: 'b', value: String::from("Frédéric") },
                ],
            },
        ];
        Self {
            leader,
            fields
        }
    }
}

