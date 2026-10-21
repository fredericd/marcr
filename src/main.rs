use std::io::{self, BufReader, BufWriter, Write, BufRead};
use std::num::NonZeroUsize;
use std::error::Error;
use std::fs::File;
use std::fmt;
use marcr::{ Format, Record, Writer, Reader };
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::process::ExitCode;

mod query;
use query::{Condition, Path};

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum ArgFormat {
    Iso2709,
    Marcxml,
    Text,
    Json,
    Mrk,
}

// Quick Display implementation to unlock default_value_t
impl fmt::Display for ArgFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArgFormat::Iso2709 => write!(f, "iso2709"),
            ArgFormat::Marcxml => write!(f, "marcxml"),
            ArgFormat::Text => write!(f, "text"),
            ArgFormat::Json => write!(f, "json"),
            ArgFormat::Mrk => write!(f, "mrk"),
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

    /// Skip the first N records of the input
    #[arg(long, value_name = "N", default_value_t = 0)]
    skip: usize,

    /// Read at most N records of the input (after --skip, before --where)
    #[arg(long, value_name = "N")]
    limit: Option<usize>,

    /// Keep only the records that have the field TAG or the subfield TAGc;
    /// with =VALUE, one with that value; with ~REGEX, one matching REGEX.
    /// A leading ! negates. Repeatable: all conditions must hold.
    /// Examples: 856, !856, 101a=fre, 700a~^Hugo
    #[arg(long = "where", value_name = "[!]TAG[c][=VALUE|~REGEX]", value_parser = Condition::parse)]
    conditions: Vec<Condition>,

    /// Write a tab-separated table instead of records: a header line,
    /// then one line per record, one column per TAG or TAGc (repeated
    /// values joined by |)
    #[arg(long, value_name = "TAG[c],...", value_delimiter = ',', value_parser = Path::parse, conflicts_with = "serialize")]
    fields: Vec<Path>,

    /// List of files containing biblio records
    #[arg(required = false)]
    files: Vec<PathBuf>,
}

/// Copies every record of `reader` that meets `conditions` to `writer`,
/// on all the threads of the current rayon pool. Records that cannot be
/// read, or written in the output format, are reported on stderr and
/// skipped.
fn write_to(
    reader: Reader<Box<dyn BufRead + Send>>,
    writer: &mut Writer<Box<dyn Write + Send>>,
    conditions: &[Condition],
    source: &str,
) -> Result<marcr::parallel::Stats, marcr::Error> {
    let keep = |record: Record| conditions.iter().all(|c| c.matches(&record)).then_some(record);
    marcr::parallel::convert(reader, writer, keep, |number, e| match e {
        // Reported with the record number in the input
        marcr::Error::Unwritable { format, message, .. } => {
            eprintln!("{source}: skipped record #{number}: cannot write {format} record: {message}");
        }
        // "record #N at byte X: malformed ... record: ..."
        e => eprintln!("{source}: skipped {e}"),
    })
}

/// Writes one TSV line per record of `reader` that meets `conditions`.
/// Malformed records are reported on stderr and skipped.
// shortcut: single-threaded, unlike write_to; run it through
// marcr::parallel if tables of whole catalogues become common.
fn write_table(
    reader: &mut Reader<Box<dyn BufRead + Send>>,
    out: &mut impl Write,
    columns: &[Path],
    conditions: &[Condition],
    source: &str,
) -> Result<marcr::parallel::Stats, Box<dyn Error>> {
    let mut stats = marcr::parallel::Stats::default();
    for result in reader {
        stats.read += 1;
        match result {
            Ok(record) if conditions.iter().all(|c| c.matches(&record)) => {
                writeln!(out, "{}", query::tsv_row(columns, &record))?;
                stats.written += 1;
            }
            Ok(_) => {}
            Err(e) if e.is_io() => return Err(e.into()),
            Err(e) => {
                eprintln!("{source}: skipped {e}");
                stats.skipped += 1;
            }
        }
    }
    Ok(stats)
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args = Args::parse();

    let get_format = |format| -> Format {
        match format {
            ArgFormat::Iso2709 => Format::Iso2709,
            ArgFormat::Marcxml => Format::Marcxml,
            ArgFormat::Text => Format::Text,
            ArgFormat::Json => Format::Json,
            ArgFormat::Mrk => Format::Mrk,
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
    let table = !args.fields.is_empty();
    // In table mode the lines are written to writer.writer directly; Text
    // adds no header or footer.
    let output_format = if table { Format::Text } else { get_format(args.serialize) };
    let mut writer = Writer::new(output_format, buf_writer);
    if table {
        writeln!(writer.writer, "{}", query::tsv_header(&args.fields))?;
    }
    let pool = match args.jobs {
        Some(jobs) => rayon::ThreadPoolBuilder::new().num_threads(jobs.get()).build()?,
        None => rayon::ThreadPoolBuilder::new().build()?, // one thread per core
    };
    let inputs: Vec<Option<&PathBuf>> = match args.files.is_empty() {
        true => vec![None], // stdin
        false => args.files.iter().map(Some).collect(),
    };
    // --skip and --limit apply to the inputs as a whole.
    let mut to_skip = args.skip;
    let mut remaining = args.limit;
    let mut written = 0;
    let mut skipped = 0;
    for input in inputs {
        if remaining == Some(0) {
            break;
        }
        let (source, input): (String, Box<dyn BufRead + Send>) = match input {
            Some(file) => (file.display().to_string(), Box::new(BufReader::with_capacity(128 * 1024, File::open(file)?))),
            None => (String::from("<stdin>"), Box::new(BufReader::with_capacity(128 * 1024, io::stdin()))),
        };
        let mut reader = Reader::new(get_format(args.deserialize), input);
        to_skip -= reader.skip_records(to_skip)?;
        if let Some(n) = remaining {
            reader.stop_after(reader.count + n);
        }
        let stats = if table {
            write_table(&mut reader, &mut writer.writer, &args.fields, &args.conditions, &source)?
        } else {
            pool.install(|| write_to(reader, &mut writer, &args.conditions, &source))?
        };
        if let Some(n) = &mut remaining {
            *n -= stats.read;
        }
        written += stats.written;
        skipped += stats.skipped;
    }
    writer.finish()?;

    if skipped > 0 {
        eprintln!("{written} records written, {skipped} skipped");
        return Ok(ExitCode::from(2));
    }
    Ok(ExitCode::SUCCESS)
}
