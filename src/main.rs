use std::io::{self, BufReader, BufWriter, Write, BufRead};
use std::error::Error;
use std::fs::File;
use std::fmt;
use marcr::{ Format, Writer, Reader };
use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ArgFormat {
    Iso2709,
    Marcxml,
    Text,
}

// Implémentation rapide de Display pour débloquer default_value_t
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
struct Args {
    /// Format of the input files: iso2709, marcxml.
    #[arg(short, long, value_name = "FORMAT", default_value_t = ArgFormat::Iso2709)]
    deserialize: ArgFormat,

    /// Format of the output file: iso2709, marxml, text.
    #[arg(short, long, value_name = "FORMAT", default_value_t = ArgFormat::Text)]
    serialize: ArgFormat,

    /// Name of the output file
    #[arg(short, long, value_name = "NAME")]
    output: Option<PathBuf>,

    /// Lits of files containing biblio records
    #[arg(required = false)]
    files: Vec<PathBuf>,
}

fn write_to(
    mut reader: Reader<Box<dyn BufRead>>,
    writer: &mut Writer<Box<dyn Write>>
) -> Result<(), Box<dyn Error>> {
    loop { 
        let result = reader.read();
        match result {
            Ok(record) => {
                let _ = match record {
                    Some(record) => writer.write(&record),
                    None => { break; },
                };
                ()
            },
            Err(e) => panic!("{e}"),
        };
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
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
    if args.files.len() > 0 {
        for file in &args.files {
            let input_file = File::open(&file)?;
            let buf_reader: Box<dyn BufRead> = Box::new(BufReader::with_capacity(128 * 1024, input_file));
            let reader = Reader::new(get_format(args.deserialize), buf_reader);
            write_to(reader, &mut writer)?;
        }
    } else {
        let stdin = io::stdin();
        let buf_reader: Box<dyn BufRead> = Box::new(BufReader::with_capacity(128 * 1024, stdin.lock()));
        let reader = Reader::new(get_format(args.serialize), buf_reader);
        write_to(reader, &mut writer)?;
    }

    Ok(())
}

