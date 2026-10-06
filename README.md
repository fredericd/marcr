# marcr

Rust library and command-line tool to read and write MARC bibliographic
records, converting between three formats:

- **ISO 2709** (`iso2709`) — the classic binary MARC exchange format.
- **MARCXML** (`marcxml`) — the standardized XML representation (Library
  of Congress).
- **Text** (`text`) — a human-readable representation, one line per
  field.

## Character encoding

`marcr` works with **UTF-8 only**: input data must be UTF-8 encoded, and
everything it writes is UTF-8. Other MARC encodings (MARC-8, ISO 5426,
ISO 8859-1…) are not converted; convert such files to UTF-8 beforehand
(with `yaz-marcdump` for instance).

Non-UTF-8 input is handled differently depending on the format:

- **ISO 2709**: invalid bytes are replaced by the replacement character
  `�` (U+FFFD), **without any warning**; the record is otherwise read.
- **MARCXML** and **text**: the record is rejected as malformed (reported
  and skipped by the command-line tool).

The leader's character coding position (09) is neither checked on input
nor updated on output; records created from scratch get the default
leader `00000nam a2200000   4500`, whose position 09 is `a` (UCS/Unicode).

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
  -j, --jobs <N>              Number of threads used to process records
                              [default: one per core]
  -h, --help                  Print help
  -V, --version                Print version
```

If no file is given, `marcr` reads from standard input. When several
files are passed as arguments, they are concatenated in the output.

Records are parsed and serialized on all cores (see
[Parallel processing](#parallel-processing)); the output is the same, in
the same order, as with a single thread (`-j 1`).

A malformed record, or one that cannot be written in the output format
(an ISO 2709 field over 9999 bytes or record over 99999 bytes), does not
stop the conversion: it is reported on standard error with its number in
the input file, and skipped.

Exit status: `0` on success, `2` if records were skipped, `1` on error
(missing file, read or write failure).

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

The command-line tool is behind the default `cli` feature. To use only
the library, without pulling in `clap`, disable default features:

```toml
[dependencies]
marcr = { version = "0.3", default-features = false }
```

### Reading and writing

A `Reader` reads records one by one from a buffered stream, a `Writer`
writes them to a stream, each in a given `Format`. Neither loads the
whole file into memory.

```rust
use marcr::{Format, Reader, Writer};
use std::fs::File;
use std::io::{BufReader, BufWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = BufReader::new(File::open("notices.mrc")?);
    let output = BufWriter::new(File::create("notices.xml")?);
    let mut reader = Reader::new(Format::Iso2709, input);
    let mut writer = Writer::new(Format::Marcxml, output);
    while let Some(record) = reader.read()? {
        writer.write(&record)?;
    }
    writer.finish()?; // closes </collection> and flushes, reporting errors
    Ok(())
}
```

This loop stops at the first error, a malformed record included; see
[Error handling](#error-handling) to skip such records instead.

Call `Writer::finish` once done: it closes the output (the MARCXML
`</collection>`) and flushes it, returning any error. Without it, the
output is still closed when the writer is dropped, but errors are lost.

A `Reader` is also an iterator over `Result<Record, marcr::Error>`:
`for result in &mut reader { ... }`. `reader.count` is the number of
records extracted from the stream so far, malformed ones included, and
`writer.count` the number of records written.

### Error handling

All fallible functions return a `marcr::Error`. Its variants tell
whether processing can go on:

| Variant | When | Context | What to do |
|---------|------|---------|------------|
| `Error::Io` | read or write error of the underlying stream | the `std::io::Error` | stop |
| `Error::Malformed` | a record cannot be parsed in the input format | `format`, `record` (number in the input), `offset` (byte where reading of it started), `message` | skip the record |
| `Error::Unwritable` | a record cannot be represented in the output format: only ISO 2709 has limits (field over 9999 bytes, record over 99999 bytes) | `format`, `record` (number in the output), `message` | skip the record |

`record` and `offset` are filled in by `Reader` and `Writer`; they are
`None` when calling `Format::deserialize` or `Format::serialize` directly.
A malformed record is consumed before its error is returned, so the next
`read` moves on to the following record; an I/O error ends the `Reader`
iteration.

The example below converts a file, skipping the records that cannot be
read or written, and stopping on an I/O error:

```rust
use marcr::{Error, Format, Reader, Writer};
use std::fs::File;
use std::io::{BufReader, BufWriter};

fn main() -> Result<(), Error> {
    // io::Error converts into Error::Io, so `?` works on file operations
    let input = BufReader::new(File::open("notices.mrc")?);
    let output = BufWriter::new(File::create("notices-copy.mrc")?);
    let mut reader = Reader::new(Format::Iso2709, input);
    let mut writer = Writer::new(Format::Iso2709, output);

    for (index, result) in (&mut reader).enumerate() {
        let number = index + 1;
        let record = match result {
            Ok(record) => record,
            Err(Error::Malformed { format, offset, message, .. }) => {
                // offset is always set for a record read through a Reader
                let offset = offset.unwrap_or_default();
                eprintln!("skipped {format} record #{number} at byte {offset}: {message}");
                continue;
            }
            // Error::Io, or a variant added in a later version: stop
            Err(e) => return Err(e),
        };
        match writer.write(&record) {
            Ok(()) => {}
            // The record is still at hand, to identify it as needed
            Err(Error::Unwritable { message, .. }) => {
                eprintln!("record #{number} not written: {message}");
            }
            Err(e) => return Err(e),
        }
    }
    writer.finish()?;
    eprintln!("{} records read, {} written", reader.count, writer.count);
    Ok(())
}
```

Things to know:

- `Error` and its `Malformed` and `Unwritable` variants are
  `#[non_exhaustive]`, so that variants and fields can be added without
  breaking your code: a `match` on `Error` needs a catch-all arm, and a
  variant pattern must end with `..`.
- `Error` displays as a full sentence, e.g. `record #12 at byte 4096:
  malformed ISO 2709 record: invalid tag`, and `error.is_io()` tells an
  I/O error apart.
- `Error` implements `std::error::Error`, so `?` also works in functions
  returning `Box<dyn std::error::Error>` or `anyhow::Result`.

### Parallel processing

With the `parallel` feature (enabled by the default `cli` feature, or
alone with `features = ["parallel"]`), `marcr::parallel::convert` reads
records with a `Reader`, filters and transforms them on all cores, and
writes them with a `Writer`, in their original order:

```rust
use marcr::{Field, Format, Reader, Writer};
use std::fs::File;
use std::io::{BufReader, BufWriter};

fn main() -> Result<(), marcr::Error> {
    let reader = Reader::new(Format::Iso2709, BufReader::new(File::open("notices.mrc")?));
    let mut writer = Writer::new(Format::Marcxml, BufWriter::new(File::create("notices.xml")?));
    let stats = marcr::parallel::convert(
        reader,
        &mut writer,
        // Runs on several threads: keep records with a 200, add a 999 to them
        |mut record| {
            record.field(200)?;
            record.add(Field::Standard(999, [' ', ' '], vec![]));
            Some(record)
        },
        // Called in input order for each skipped record, with its number
        // in the input; a Malformed error already displays it
        |_number, error| eprintln!("skipped {error}"),
    )?;
    writer.finish()?;
    eprintln!("{} read, {} written, {} skipped", stats.read, stats.written, stats.skipped);
    Ok(())
}
```

- The function passed as third argument filters (`None` drops the
  record) and transforms records; it runs on several threads at once.
- Malformed and unwritable records are passed to the last argument with
  their number in the input, and skipped; an `Error::Io` stops processing
  and is returned. The input number is useful for `Error::Unwritable`,
  whose own `record` field is the number in the output.
- The output is the same, byte for byte, as with a sequential loop over
  `Reader::read` and `Writer::write`. Memory use stays bounded (a few
  batches of 4 MB in flight), whatever the input size.
- Work runs on the current [rayon](https://docs.rs/rayon) thread pool:
  one thread per core by default, or `RAYON_NUM_THREADS`; call `convert`
  inside `rayon::ThreadPool::install` to use a custom pool.

On a 1 GB file (Apple M5, 10 cores), ISO 2709 → MARCXML takes 1.9 s
instead of 9.5 s with one thread, ISO 2709 → ISO 2709 0.55 s instead of
2.8 s.

### Accessing fields

`Record` directly exposes its fields (`leader`, `fields`) as well as
utility methods (`add`, `insert`) to build a record programmatically.

To access fields by tag, `field` returns the first field with a given
tag (`None` if there is none), `fields_by_tag` iterates over all of them,
and `remove_tag` removes and returns them. `field_mut` and
`fields_by_tag_mut` give mutable access to modify fields in place:

```rust
use marcr::{Field, Record, Subfield};

fn main() {
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

    // Value of the first $a subfield of the first 200 field
    if let Some(title) = record.field(200).and_then(|field| field.subfield('a')) {
        println!("{title}");
    }

    // Add a $2 subfield to every 610 field
    for field in record.fields_by_tag_mut(610) {
        if let Field::Standard(_, _, subfields) = field {
            subfields.push(Subfield('2', String::from("rameau")));
        }
    }
}
```

`Field::subfield` returns the value of the first subfield with a given
code as a `&str` borrowed from the record (`None` if there is none, or for
a control field). Call `.to_string()` on it to keep the value beyond the
record's lifetime.

## Benchmark

`bench/bench.sh` compares the `marcr` command-line tool with
[`yaz-marcdump`](https://software.indexdata.com/yaz/) on four
conversions, from a UTF-8 ISO 2709 file (a MARCXML copy is generated next
to it for the MARCXML input case):

```sh
bench/bench.sh notices.mrc [RUNS]   # RUNS defaults to 3
```

Reference results on a 1 GB extract of a BnF export (717,104 records),
Apple M5 (10 cores), YAZ 5.37.3, median of 3 runs, output discarded.
`marcr` uses all cores by default, `yaz-marcdump` a single one; the
`marcr -j 1` column gives the single-thread figures:

| Conversion           | marcr   | marcr -j 1 | yaz-marcdump | Peak memory (marcr / yaz) |
|----------------------|---------|------------|--------------|---------------------------|
| ISO 2709 → ISO 2709  | 0.56 s  | 2.82 s     | 4.86 s       | 46 / 8.3 MB               |
| ISO 2709 → MARCXML   | 1.99 s  | 9.48 s     | 13.59 s      | 68 / 8.3 MB               |
| ISO 2709 → text      | 0.55 s  | 2.90 s     | 3.41 s       | 47 / 8.2 MB               |
| MARCXML → ISO 2709   | 3.37 s  | 9.61 s     | 25.11 s      | 23 / 10.7 MB              |

The MARCXML input is the same extract converted to MARCXML (3.4 GB).
Memory stays bounded whatever the input size: a few 4 MB batches of
records are in flight between threads.

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
src/error.rs          marcr::Error
src/reader.rs         Reader
src/writer.rs         Writer
src/tests.rs          Library unit tests
src/main.rs           Command-line interface (clap)
src/parallel.rs       marcr::parallel, multi-core processing (feature "parallel")
tests/cli.rs          Binary integration tests
bench/bench.sh        Benchmark against yaz-marcdump
```
