use std::fmt;
use std::str;

use crate::DEFAULT_LEADER;

/// Un sous-champ d'un champ MARC "standard" (tag ≥ 10) : un code d'un
/// caractère (`$a`, `$b`, ...) et sa valeur.
#[derive(Debug)]
pub struct Subfield(pub char, pub String);

impl fmt::Display for Subfield {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Subfield(letter, value) = &self;
        write!(f, "${} {}", *letter as char, value)
    }
}

/// Un champ d'une notice MARC : soit un champ de contrôle (tag < 10, une
/// simple valeur texte), soit un champ standard (tag ≥ 10, deux
/// indicateurs et une liste de sous-champs).
#[derive(Debug)]
pub enum Field {
    /// Champ de contrôle (tags 001-009) : tag et valeur.
    Control(u16, String),
    /// Champ standard (tags ≥ 010) : tag, les deux indicateurs, et les
    /// sous-champs.
    Standard(u16, [char; 2], Vec<Subfield>),
}

impl Field {
    /// Le tag du champ (001-999).
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
            Field::Control(tag, value) => write!(f, "{tag:03} {value}"),
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

/// Une notice bibliographique MARC : un leader de 24 octets et une liste
/// de champs.
#[derive(Debug)]
pub struct Record {
    /// Les 24 octets du leader (longueur de la notice, statut, type de
    /// document, position/taille du répertoire, etc.).
    pub leader: [u8; 24],
    /// Les champs de la notice, triés par tag croissant lorsqu'ils sont
    /// ajoutés via [`Record::add`] ou [`Record::insert`].
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
    /// Insère `field` en conservant l'ordre croissant des tags (tri par
    /// insertion : les champs de même tag gardent leur ordre relatif).
    pub fn add(&mut self, field: Field) {
        let tag = field.tag();
        let pos: usize = self.fields.iter()
            .position(|item| item.tag() > tag)
            .unwrap_or(self.fields.len());
        self.fields.insert(pos, field);
    }

    /// Construit une notice avec le leader par défaut et les champs
    /// donnés, sans les trier (contrairement à [`Record::add`]).
    pub fn new(fields: Vec<Field>) -> Self {
        let leader: [u8; 24] = DEFAULT_LEADER;
        Self {
            leader,
            fields,
        }
    }

    /// Construit et ajoute des champs à partir d'une notation compacte :
    /// chaque `Vec<&str>` est `[tag, indicateurs_ou_valeur, code1, valeur1,
    /// code2, valeur2, ...]`.
    ///
    /// - Pour un champ de contrôle (tag ≤ 9) : `vec!["001", "PPN1234"]`.
    /// - Pour un champ standard : `vec!["200", "  ", "a", "Titre", "b", "Sous-titre"]`
    ///   où `"  "` sont les deux indicateurs.
    ///
    /// Les entrées trop courtes ou mal formées sont silencieusement
    /// ignorées. Les champs sont ajoutés triés par tag via [`Record::add`].
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
