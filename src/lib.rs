use std::fmt;

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
        tag: u16,
        value: String,
    },
    Standard {
        tag: u16,
        ind: [char; 2],
        subfs: Vec<Subfield>,
    },
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::Control { tag, value } => write!(f, "{tag:03}    {value}"),
            Field::Standard { tag, ind, subfs } => {
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
}

