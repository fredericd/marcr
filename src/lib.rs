use std::fmt;


#[derive(Debug)]
pub struct Tag(u16);

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.0;
        let text = format!("{value:03}");
        write!(f, "{text}")
    }
}

#[derive(Debug)]
pub struct ControlField {
    pub tag: Tag,
    pub value: String,
}

impl fmt::Display for ControlField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = &self.tag;
        let value = &self.value;
        write!(f, "{tag}    {value}")
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

pub struct StandardField {
    pub tag: Tag,
    pub ind: [char; 2],
    pub subfs: Vec<Subfield>,
}

impl fmt::Display for StandardField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = &self.tag;
        let ind = &self.ind;
        let ind1 = ind[0];
        let ind2 = ind[1];
        let subfs = &self.subfs;
        write!(f, "{tag} {ind1}{ind2} ")?;
        for s in subfs {
            write!(f, "{s} ")?;
        }
        Ok(())
    }
}

pub enum Field {
    Control(ControlField),
    Standard(StandardField),
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::Control(field) => write!(f, "{field}"),
            Field::Standard(field) => write!(f, "{field}"),
        }
    }
}

pub struct Record {
    pub leader: [u8; 3],
    pub fields: Vec<Field>,
}

impl Record {
    pub fn new() -> Self {
        let leader = [1,1,1];
        let fields = vec![
            Field::Control(ControlField {tag: Tag(001), value: String::from("000001")}),
            Field::Control(ControlField {tag: Tag(005), value: String::from("2026")}),
            Field::Standard(StandardField{
                tag: Tag(200),
                ind: [' ', '1'],
                subfs: vec![
                    Subfield { letter: 'a', value: String::from("Mon titre") },
                    Subfield { letter: 'e', value: String::from("Complément du titre") },
                ],
            }),
            Field::Standard(StandardField{
                tag: Tag(700),
                ind: [' ', '1'],
                subfs: vec![
                    Subfield { letter: 'a', value: String::from("Demians") },
                    Subfield { letter: 'b', value: String::from("Frédéric") },
                ],
            }),
        ];
        Self {
            leader,
            fields
        }
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for field in &self.fields {
            writeln!(f, "{field}");
        }
        Ok(())
    }
}
