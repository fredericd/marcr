use crate::Record;

mod iso2709;
mod marcxml;
mod text;

/// Un format de notice MARC, utilisé pour la (dé)sérialisation via
/// [`Format::serialize`]/[`Format::deserialize`] ou par [`crate::Reader`]/[`crate::Writer`].
#[derive(PartialEq)]
pub enum Format {
  /// Format d'échange binaire ISO 2709 (extension `.mrc`). Lecture et
  /// écriture supportées.
  Iso2709,
  /// MARCXML, le schéma XML de la Library of Congress (extension `.xml`).
  /// Lecture et écriture supportées.
  Marcxml,
  /// Format texte lisible, une ligne par champ (extension `.txt`).
  /// Lecture et écriture supportées ; [`crate::Reader`] sépare les notices sur
  /// une ligne vide (le séparateur produit par [`crate::Writer`] pour ce format).
  Text,
}

/// Métadonnées descriptives d'un [`Format`], utilisées par exemple pour
/// peupler une liste déroulante ou choisir une extension de fichier.
#[allow(dead_code)]
pub struct RWDescription {
    pub format: Format,
    pub description: String,
    pub extension: String,
}

impl Format {
    /// La liste des formats supportés avec leur description et leur
    /// extension de fichier usuelle.
    pub fn get_available_formats() -> Vec<RWDescription> {
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

    /// Parse `octets` (une notice complète, sans octets superflus avant
    /// ou après) selon `self`. Voir [`crate::Reader`] pour lire des notices en
    /// série depuis un flux plus large.
    ///
    /// Retourne une erreur si `octets` n'est pas une notice valide dans ce
    /// format, ou toujours une erreur pour [`Format::Text`] (écriture
    /// uniquement).
    pub fn deserialize(&self, octets: &[u8]) -> Result<Record, Box<dyn std::error::Error>> {
        match self {
            Format::Iso2709 => self.deserialize_iso2709(octets),
            Format::Marcxml => self.deserialize_marcxml(octets),
            Format::Text => self.deserialize_text(octets),
        }
    }

    /// Sérialise `record` selon `self`. Pour écrire plusieurs notices vers
    /// un flux (en-tête/pied MARCXML, séparateurs), préférer [`crate::Writer`].
    pub fn serialize(&self, record: &Record) -> Vec<u8> {
        match self {
            Format::Iso2709 => self.serialize_iso2709(record),
            Format::Marcxml => self.serialize_marcxml(record),
            Format::Text => self.serialize_text(record),
        }
    }
}
