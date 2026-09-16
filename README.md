# marcr

Bibliothèque et outil en ligne de commande Rust pour lire et écrire des
notices bibliographiques MARC, avec conversion entre trois formats :

- **ISO 2709** (`iso2709`) — le format d'échange MARC binaire classique.
- **MARCXML** (`marcxml`) — la représentation XML normalisée (Library of
  Congress).
- **Texte** (`text`) — une représentation lisible, une ligne par champ.
  Ce format n'est disponible qu'en **sortie** (pas de parseur en entrée).

## Compilation

```sh
cargo build --release
```

Le binaire est produit dans `target/release/marcr`. Le profil `release`
est réglé pour la vitesse d'exécution (`opt-level = 3`, LTO activé)
plutôt que pour la taille du binaire, car l'outil est destiné à traiter
des fichiers de plusieurs centaines de Mo à quelques Go.

## Utilisation en ligne de commande

```
marcr [OPTIONS] [FICHIERS]...

Options:
  -d, --deserialize <FORMAT>  Format des fichiers d'entrée [défaut: iso2709]
                              [valeurs possibles: iso2709, marcxml, text]
  -s, --serialize <FORMAT>    Format du fichier de sortie [défaut: text]
                              [valeurs possibles: iso2709, marcxml, text]
  -o, --output <NOM>          Nom du fichier de sortie (sinon stdout)
  -h, --help                  Affiche l'aide
  -V, --version               Affiche la version
```

Si aucun fichier n'est indiqué, `marcr` lit sur l'entrée standard. Quand
plusieurs fichiers sont passés en argument, ils sont concaténés dans la
sortie.

### Exemples

Convertir un fichier ISO2709 en MARCXML :

```sh
marcr -d iso2709 -s marcxml -o notices.xml notices.mrc
```

Afficher en texte lisible le contenu d'un fichier MARCXML sur la sortie
standard :

```sh
marcr -d marcxml -s text notices.xml
```

Utilisation avec un pipe (entrée standard, format par défaut ISO2709) :

```sh
cat notices.mrc | marcr -s marcxml > notices.xml
```

Fusionner plusieurs fichiers ISO2709 en un seul :

```sh
marcr -d iso2709 -s iso2709 -o fusion.mrc a.mrc b.mrc c.mrc
```

## Utilisation en tant que bibliothèque

```rust
use marcr::{Format, Reader, Writer};
use std::io::{BufReader, Cursor};

let data: &[u8] = b"..."; // notices au format ISO2709
let mut reader = Reader::new(Format::Iso2709, BufReader::new(Cursor::new(data)));

let mut writer = Writer::new(Format::Marcxml, std::io::stdout());
while let Some(record) = reader.read()? {
    writer.write(&record)?;
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Record` expose directement ses champs (`leader`, `fields`) ainsi que des
méthodes utilitaires (`add`, `insert`) pour construire une notice par
programme.

## Tests

```sh
cargo test --lib          # tests unitaires (src/tests.rs)
cargo test --test cli     # tests d'intégration du binaire (tests/cli.rs)
```

## Structure du projet

```
src/lib.rs      Format (Iso2709/Marcxml/Text), Record, Reader, Writer
src/tests.rs    Tests unitaires de la bibliothèque
src/main.rs     Interface en ligne de commande (clap)
tests/cli.rs    Tests d'intégration du binaire
```
