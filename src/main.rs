use std::io::{self, BufReader, BufWriter, Write, BufRead};
use std::error::Error;
use std::fs::File;
use std::fmt;
use marcr::{ Format, Writer, Reader };
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ArgFormat {
    Iso2709,
    Marcxml,
    Text,
}

// Quick Display implementation to unlock default_value_t
impl fmt::Display for ArgFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgFormat::Iso2709 => write!(f, "iso2709"),
            ArgFormat::Marcxml => write!(f, "marcxml"),
            ArgFormat::Text => write!(f, "text"),
        }
    }
}

/// Read/write MARC biblio records files in various formats
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
#[command(after_help = "Exit status: 0 on success, 2 if malformed records were skipped, 1 on error.")]
struct Args {
    /// Format of the input files
    #[arg(short, long, value_name = "FORMAT", default_value_t = ArgFormat::Iso2709)]
    deserialize: ArgFormat,

    /// Format of the output file
    #[arg(short, long, value_name = "FORMAT", default_value_t = ArgFormat::Text)]
    serialize: ArgFormat,

    /// Name of the output file
    #[arg(short, long, value_name = "NAME")]
    output: Option<PathBuf>,

    /// Lits of files containing biblio records
    #[arg(required = false)]
    files: Vec<PathBuf>,
}

/// Copies every record of `reader` to `writer`. Records that cannot be
/// read, or written in the output format, are reported on stderr and
/// skipped; returns how many were skipped.
fn write_to(
    mut reader: Reader<Box<dyn BufRead>>,
    writer: &mut Writer<Box<dyn Write>>,
    source: &str,
) -> Result<usize, Box<dyn Error>> {
    let mut skipped = 0;
    loop {
        match reader.read() {
            Ok(Some(record)) => match writer.write(&record) {
                Ok(()) => (),
                Err(e) if e.is::<io::Error>() => return Err(e),
                Err(e) => {
                    skipped += 1;
                    eprintln!("{source}: skipped record #{} on output: {e}", reader.count);
                }
            },
            Ok(None) => break,
            // An I/O error is not tied to one record: stop there.
            Err(e) if e.is::<io::Error>() => return Err(e),
            Err(e) => {
                skipped += 1;
                eprintln!("{source}: skipped record #{}: {e}", reader.count);
            }
        }
    }
    Ok(skipped)
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = Args::parse();

    let get_format = |format| -> Format {
        match format {
            ArgFormat::Iso2709 => Format::Iso2709,
            ArgFormat::Marcxml => Format::Marcxml,
            ArgFormat::Text => Format::Text,
        }
    };

    let buf_writer: Box<dyn Write> = match args.output.as_deref() {
        Some(output) => {
            let output_file = File::create(output)?;
            Box::new(BufWriter::new(output_file))
        },
        None => {
            let stdout = io::stdout();
            Box::new(BufWriter::new(stdout))
        },
    };
    let output_format = get_format(args.serialize);
    let mut writer = Writer::new(output_format, buf_writer);
    let mut skipped = 0;
    if args.files.len() > 0 {
        for file in &args.files {
            let input_file = File::open(&file)?;
            let buf_reader: Box<dyn BufRead> = Box::new(BufReader::with_capacity(128 * 1024, input_file));
            let reader = Reader::new(get_format(args.deserialize), buf_reader);
            skipped += write_to(reader, &mut writer, &file.display().to_string())?;
        }
    } else {
        let stdin = io::stdin();
        let buf_reader: Box<dyn BufRead> = Box::new(BufReader::with_capacity(128 * 1024, stdin.lock()));
        let reader = Reader::new(get_format(args.deserialize), buf_reader);
        skipped += write_to(reader, &mut writer, "<stdin>")?;
    }
    // Report write errors instead of losing them when the buffer is
    // flushed on drop.
    writer.writer.flush()?;

    if skipped > 0 {
        eprintln!("{} records written, {skipped} skipped", writer.count);
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}

