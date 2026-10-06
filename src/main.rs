use std::io::{self, BufReader, BufWriter, Write, BufRead};
use std::num::NonZeroUsize;
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

    /// Number of threads used to process records [default: one per core]
    #[arg(short, long, value_name = "N")]
    jobs: Option<NonZeroUsize>,

    /// List of files containing biblio records
    #[arg(required = false)]
    files: Vec<PathBuf>,
}

/// Copies every record of `reader` to `writer`, on all the threads of the
/// current rayon pool. Records that cannot be read, or written in the
/// output format, are reported on stderr and skipped; returns how many
/// were skipped.
fn write_to(
    reader: Reader<Box<dyn BufRead + Send>>,
    writer: &mut Writer<Box<dyn Write + Send>>,
    source: &str,
) -> Result<usize, marcr::Error> {
    let stats = marcr::parallel::convert(reader, writer, Some, |number, e| match e {
        // Reported with the record number in the input
        marcr::Error::Unwritable { format, message, .. } => {
            eprintln!("{source}: skipped record #{number}: cannot write {format} record: {message}");
        }
        // "record #N at byte X: malformed ... record: ..."
        e => eprintln!("{source}: skipped {e}"),
    })?;
    Ok(stats.skipped)
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

    let buf_writer: Box<dyn Write + Send> = match args.output.as_deref() {
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
    let pool = match args.jobs {
        Some(jobs) => rayon::ThreadPoolBuilder::new().num_threads(jobs.get()).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?, // one thread per core
    };
    let mut skipped = 0;
    if args.files.len() > 0 {
        for file in &args.files {
            let input_file = File::open(&file)?;
            let buf_reader: Box<dyn BufRead + Send> = Box::new(BufReader::with_capacity(128 * 1024, input_file));
            let reader = Reader::new(get_format(args.deserialize), buf_reader);
            let source = file.display().to_string();
            skipped += pool.install(|| write_to(reader, &mut writer, &source))?;
        }
    } else {
        let buf_reader: Box<dyn BufRead + Send> = Box::new(BufReader::with_capacity(128 * 1024, io::stdin()));
        let reader = Reader::new(get_format(args.deserialize), buf_reader);
        skipped += pool.install(|| write_to(reader, &mut writer, "<stdin>"))?;
    }
    writer.finish()?;

    if skipped > 0 {
        eprintln!("{} records written, {skipped} skipped", writer.count);
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}

