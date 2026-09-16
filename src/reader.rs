use std::io::{BufRead, Read};

use memchr::memchr;

use crate::{Format, Record, RT, XML_END_TAG, XML_START_PREFIX};

/// Lit des notices en série depuis un flux `R`, dans un format donné.
///
/// Chaque appel à [`Reader::read`] extrait et parse la notice suivante,
/// sans avoir à charger tout le flux en mémoire. Pour `format: Text`, les
/// notices doivent être séparées par une ligne vide (c'est ce que produit
/// [`crate::Writer`] pour ce format).
pub struct Reader<R> {
    pub format: Format,
    pub reader: R,
    pub buffer: Vec<u8>,   // Reusable internal buffer
    pub count: usize,
}

impl<R: Read + BufRead> Reader<R> {
    /// Crée un lecteur pour `format` au-dessus du flux bufferisé `reader`.
    pub fn new(format: Format, reader: R) -> Self {
        let buffer: Vec<u8> = Vec::new();
        let count = 0;
        Self { format, reader, buffer, count }
    }

    /// Lit et parse la prochaine notice du flux.
    ///
    /// Retourne `Ok(None)` en fin de flux, `Err` en cas d'erreur de
    /// lecture ou de notice mal formée.
    pub fn read(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        match self.format {
            Format::Iso2709 => self.read_iso2709(),
            Format::Marcxml => self.read_marcxml(),
            Format::Text    => self.read_text(),
        }
    }

    /// Lit la prochaine notice ISO 2709 : cherche l'octet RT (`0x1d`) qui
    /// la termine dans le flux, en accumulant dans un tampon interne si
    /// elle s'étend sur plusieurs lectures du `BufReader`.
    pub fn read_iso2709(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        // Tampon de secours uniquement pour les notices plus grandes que la taille du BufReader
        self.buffer.clear();
        let mut found = false;
        let mut option_record: Option<Record> = None;
        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // End of reader without finding RT. Return None
                break;
            }
            let consumed = {
                if let Some(pos) = memchr(RT, available) {
                    if self.buffer.is_empty() {
                        // Found in reader buffer. No need to use an internal buffer, ie zero-copy
                        let octets = &available[..=pos];
                        option_record = Some(self.format.deserialize(octets)?);
                    } else {
                        // Found a record which was extended on several reader buffer
                        self.buffer.extend_from_slice(&available[..=pos]);
                        option_record = Some(self.format.deserialize(&self.buffer)?);
                    }
                    found = true;
                    pos + 1
                } else {
                    self.buffer.extend_from_slice(available);
                    available.len()
                }
            };
            self.reader.consume(consumed);
        }
        Ok(option_record)
    }

    /// Lit la prochaine notice MARCXML : cherche l'élément `<record>`
    /// suivant (avec ou sans attributs, ex. `xmlns="..."`) puis capture
    /// jusqu'à `</record>` inclus.
    pub fn read_marcxml(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        self.buffer.clear();
        let mut found = false;
        let mut option_record: Option<Record> = None;

        // Repérage de <record> : on matche le préfixe "<record", puis on
        // vérifie que l'octet suivant est bien une limite de nom de balise
        // (espace/tab/EOL ou '>'), afin d'accepter <record>, <record ...>
        // ou <record xmlns="...">, sans matcher par erreur <records>.
        enum State { Searching, OpenTag, Content }
        let mut state = State::Searching;
        let mut matched_start = 0; // progression dans XML_START_PREFIX
        let mut matched_end = 0;   // progression dans XML_END_TAG

        while !found {
            // 1. Accès direct au tampon mémoire du BufReader
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                break; // Fin de fichier propre
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                match state {
                    State::Searching => {
                        if b == XML_START_PREFIX[matched_start] {
                            matched_start += 1;
                            if matched_start == XML_START_PREFIX.len() {
                                self.buffer.extend_from_slice(XML_START_PREFIX);
                                state = State::OpenTag;
                            }
                        } else if b == XML_START_PREFIX[0] {
                            matched_start = 1;
                        } else {
                            matched_start = 0;
                        }
                    },
                    State::OpenTag => {
                        if self.buffer.len() == XML_START_PREFIX.len() && !(b.is_ascii_whitespace() || b == b'>') {
                            // Faux positif (ex: "<records") : on annule et on
                            // reprend la recherche à partir de cet octet.
                            self.buffer.clear();
                            state = State::Searching;
                            matched_start = if b == XML_START_PREFIX[0] { 1 } else { 0 };
                            continue;
                        }
                        self.buffer.push(b);
                        if b == b'>' {
                            state = State::Content;
                        }
                    },
                    State::Content => {
                        self.buffer.push(b);
                        if b == XML_END_TAG[matched_end] {
                            matched_end += 1;
                            if matched_end == XML_END_TAG.len() {
                                found = true;
                                option_record = Some(self.format.deserialize(&self.buffer)?);
                                break;
                            }
                        } else if b == XML_END_TAG[0] {
                            matched_end = 1;
                        } else {
                            matched_end = 0;
                        }
                    },
                }
            }

            // 2. Informe le BufReader qu'on a traité `consumed` octets
            self.reader.consume(consumed);
        }
        Ok(option_record)
    }

    /// Lit la prochaine notice texte : accumule les octets jusqu'à une
    /// ligne vide (deux octets `\n` consécutifs), qui sépare deux notices
    /// — c'est le séparateur produit par [`crate::Writer`] pour ce format. La
    /// dernière notice du flux, non suivie d'une ligne vide, est acceptée
    /// à la fin du flux (EOF) s'il reste du contenu accumulé.
    pub fn read_text(&mut self) -> Result<Option<Record>, Box<dyn std::error::Error>> {
        self.buffer.clear();
        let mut found = false;
        let mut option_record: Option<Record> = None;

        while !found {
            let available = self.reader.fill_buf()?;
            if available.is_empty() {
                // Fin de flux : la dernière notice n'est suivie d'aucune
                // ligne vide, on décode ce qu'il reste s'il y en a.
                if !self.buffer.is_empty() {
                    option_record = Some(self.format.deserialize(&self.buffer)?);
                }
                break;
            }

            let mut consumed = 0;
            for &b in available {
                consumed += 1;
                if b == b'\n' && self.buffer.last() == Some(&b'\n') {
                    // Deux '\n' consécutifs : ligne vide séparatrice, pas
                    // incluse dans la notice.
                    found = true;
                    option_record = Some(self.format.deserialize(&self.buffer)?);
                    break;
                }
                self.buffer.push(b);
            }
            self.reader.consume(consumed);
        }
        Ok(option_record)
    }
}
