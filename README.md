# marcr

Rust library and command-line tool to read and write MARC bibliographic
records, converting between three formats:

- **ISO 2709** (`iso2709`) — the classic binary MARC exchange format.
- **MARCXML** (`marcxml`) — the standardized XML representation (Library
  of Congress).
- **Text** (`text`) — a human-readable representation, one line per
  field.

## Build

```sh
cargo build --release
```

The binary is produced at `target/release/marcr`. The `release` profile
is tuned for execution speed (`opt-level = 3`, LTO enabled) rather than
binary size, since the tool is meant to process files from several
hundred MB up to a few GB.

## Command-line usage

```
marcr [OPTIONS] [FILES]...

Options:
  -d, --deserialize <FORMAT>  Format of the input files [default: iso2709]
                              [possible values: iso2709, marcxml, text]
  -s, --serialize <FORMAT>    Format of the output file [default: text]
                              [possible values: iso2709, marcxml, text]
  -o, --output <NAME>         Name of the output file (stdout otherwise)
  -h, --help                  Print help
  -V, --version                Print version
```

If no file is given, `marcr` reads from standard input. When several
files are passed as arguments, they are concatenated in the output.

### Examples

Convert an ISO2709 file to MARCXML:

```sh
marcr -d iso2709 -s marcxml -o notices.xml notices.mrc
```

Print the content of a MARCXML file as human-readable text on standard
output:

```sh
marcr -d marcxml -s text notices.xml
```

Usage with a pipe (standard input, default ISO2709 format):

```sh
cat notices.mrc | marcr -s marcxml > notices.xml
```

Merge several ISO2709 files into one:

```sh
marcr -d iso2709 -s iso2709 -o merged.mrc a.mrc b.mrc c.mrc
```

## Library usage

```rust
use marcr::{Format, Reader, Writer};
use std::io::{BufReader, Cursor};

let data: &[u8] = b"..."; // ISO2709-formatted records
let mut reader = Reader::new(Format::Iso2709, BufReader::new(Cursor::new(data)));

let mut writer = Writer::new(Format::Marcxml, std::io::stdout());
while let Some(record) = reader.read()? {
    writer.write(&record)?;
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Reader::read` returns an error on a malformed record, but consumes it
first: calling `read` again moves on to the next record. This makes it
possible to skip unreadable records instead of stopping:

```rust
use marcr::{Format, Reader, Writer};
use std::io::{BufReader, Cursor};

let data: &[u8] = b"..."; // ISO2709-formatted records
let mut reader = Reader::new(Format::Iso2709, BufReader::new(Cursor::new(data)));

let mut writer = Writer::new(Format::Iso2709, std::io::stdout());
loop {
    match reader.read() {
        Ok(Some(record)) => writer.write(&record)?,
        Ok(None) => break,
        // An I/O error is not tied to a record: stop there.
        Err(err) if err.is::<std::io::Error>() => return Err(err),
        Err(err) => eprintln!("skipped record: {err}"),
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Record` directly exposes its fields (`leader`, `fields`) as well as
utility methods (`add`, `insert`) to build a record programmatically.

To access fields by tag, `field` returns the first field with a given
tag (`None` if there is none), `fields_by_tag` iterates over all of them,
and `remove_tag` removes and returns them:

```rust
use marcr::{Field, Record};

let mut record = Record::default();
record.insert(vec![
    vec!["001", "PPN1234"],
    vec!["200", " 1", "a", "Mon titre"],
    vec!["610", "  ", "a", "Sujet 1"],
    vec!["610", "  ", "a", "Sujet 2"],
]);

// First occurrence of a tag
match record.field(200) {
    Some(Field::Standard(_, indicators, subfields)) => {
        println!("200 {indicators:?}: {}", subfields[0].1);
    }
    Some(Field::Control(_, value)) => println!("{value}"), // tags 001-009
    None => println!("no 200 field"),
}

// All occurrences of a tag
for field in record.fields_by_tag(610) {
    println!("{field}");
}
```

## Tests

```sh
cargo test --lib          # library unit tests (src/tests.rs)
cargo test --test cli     # binary integration tests (tests/cli.rs)
```

## Project structure

```
src/lib.rs           Crate doc, shared constants, module re-exports
src/record.rs         Subfield, Field, Record
src/format/mod.rs     Format enum, RWDescription, serialize/deserialize dispatch
src/format/iso2709.rs ISO 2709 (de)serialization
src/format/marcxml.rs MARCXML (de)serialization
src/format/text.rs    Text format (de)serialization
src/reader.rs         Reader
src/writer.rs         Writer
src/tests.rs          Library unit tests
src/main.rs           Command-line interface (clap)
tests/cli.rs          Binary integration tests
```
