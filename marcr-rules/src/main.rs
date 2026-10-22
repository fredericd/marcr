use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use marcr::{Field, Format, Reader, Record, Subfield, Writer};
use marcr_rules::{Rules, BUILTIN};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ArgFormat {
    Iso2709,
    Marcxml,
    Text,
    Json,
    Mrk,
}

impl From<ArgFormat> for Format {
    fn from(format: ArgFormat) -> Format {
        match format {
            ArgFormat::Iso2709 => Format::Iso2709,
            ArgFormat::Marcxml => Format::Marcxml,
            ArgFormat::Text => Format::Text,
            ArgFormat::Json => Format::Json,
            ArgFormat::Mrk => Format::Mrk,
        }
    }
}

/// Convert MARC records with a rules file
#[derive(Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Convert records
    Convert {
        /// Rules: a file, or the name of built-in rules (see `list`)
        #[arg(short, long, value_name = "FILE|NAME")]
        rules: String,
        /// Format of the input files
        #[arg(short, long, value_name = "FORMAT", default_value = "iso2709")]
        deserialize: ArgFormat,
        /// Format of the output file
        #[arg(short, long, value_name = "FORMAT", default_value = "text")]
        serialize: ArgFormat,
        /// Name of the output file (stdout otherwise)
        #[arg(short, long, value_name = "NAME")]
        output: Option<PathBuf>,
        /// Number of threads [default: one per core]
        #[arg(short, long, value_name = "N")]
        jobs: Option<NonZeroUsize>,
        /// Input files (stdin otherwise)
        files: Vec<PathBuf>,
    },
    /// Convert SOURCE records and compare them, tag by tag, with the
    /// EXPECTED records having the same 001
    Eval {
        /// Rules: a file, or the name of built-in rules (see `list`)
        #[arg(short, long, value_name = "FILE|NAME")]
        rules: String,
        /// Format of both files
        #[arg(short, long, value_name = "FORMAT", default_value = "iso2709")]
        deserialize: ArgFormat,
        /// Show differences for these tags (comma-separated)
        #[arg(long, value_name = "TAG,...", value_delimiter = ',')]
        show: Vec<String>,
        /// Number of differences shown per tag
        #[arg(long, value_name = "N", default_value_t = 5)]
        examples: usize,
        /// Pairs records listed in FILE (lines "SOURCE_001 EXPECTED_001")
        /// instead of by equal 001
        #[arg(long, value_name = "FILE")]
        pairs: Option<PathBuf>,
        source: PathBuf,
        expected: PathBuf,
    },
    /// List the built-in rules
    List,
    /// Print built-in rules, to start a rules file of one's own
    Show {
        name: String,
    },
}

/// Compiles `rules`: an existing file, else built-in rules of that name.
fn load(rules: &str) -> Result<Rules, Box<dyn Error>> {
    if std::path::Path::new(rules).exists() {
        return Ok(Rules::from_file(rules)?);
    }
    Rules::builtin(rules).ok_or_else(|| format!("{rules}: no such file or built-in rules").into())
}

fn main() -> Result<ExitCode, Box<dyn Error>> {
    match Args::parse().command {
        Command::Convert { rules, deserialize, serialize, output, jobs, files } => {
            convert(&load(&rules)?, deserialize.into(), serialize.into(), output, jobs, &files)
        }
        Command::Eval { rules, deserialize, show, examples, pairs, source, expected } => {
            eval(&load(&rules)?, deserialize.into(), &show, examples, pairs.as_ref(), &source, &expected)?;
            Ok(ExitCode::SUCCESS)
        }
        Command::List => {
            for (name, _) in BUILTIN {
                let rules = Rules::builtin(name).unwrap();
                println!("{name}\t{} {}", rules.name().unwrap_or(""), rules.version().unwrap_or(""));
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Show { name } => match BUILTIN.iter().find(|(n, _)| *n == name) {
            Some((_, text)) => {
                print!("{text}");
                Ok(ExitCode::SUCCESS)
            }
            None => Err(format!("{name}: no such built-in rules").into()),
        },
    }
}

fn convert(
    rules: &Rules,
    input_format: Format,
    output_format: Format,
    output: Option<PathBuf>,
    jobs: Option<NonZeroUsize>,
    files: &[PathBuf],
) -> Result<ExitCode, Box<dyn Error>> {
    let out: Box<dyn Write + Send> = match output {
        Some(path) => Box::new(BufWriter::new(File::create(path)?)),
        None => Box::new(BufWriter::new(io::stdout())),
    };
    let mut writer = Writer::new(output_format, out);
    let mut pool = rayon::ThreadPoolBuilder::new();
    if let Some(jobs) = jobs {
        pool = pool.num_threads(jobs.get());
    }
    let pool = pool.build()?;
    let inputs: Vec<Option<&PathBuf>> = if files.is_empty() { vec![None] } else { files.iter().map(Some).collect() };
    let mut skipped = 0;
    for input in inputs {
        let (source, input): (String, Box<dyn BufRead + Send>) = match input {
            Some(file) => (file.display().to_string(), Box::new(BufReader::new(File::open(file)?))),
            None => (String::from("<stdin>"), Box::new(BufReader::new(io::stdin()))),
        };
        let reader = Reader::new(input_format, input);
        let stats = pool.install(|| {
            marcr::parallel::convert(reader, &mut writer, |record| Some(rules.apply(&record)), |_, e| {
                eprintln!("{source}: skipped {e}");
            })
        })?;
        skipped += stats.skipped;
    }
    writer.finish()?;
    Ok(if skipped > 0 { ExitCode::from(2) } else { ExitCode::SUCCESS })
}

/// Per tag counts of `eval`, in records.
#[derive(Default)]
struct TagStats {
    expected: usize,
    produced: usize,
    equal: usize,
    /// (001, expected lines, produced lines)
    differences: Vec<(String, Vec<String>, Vec<String>)>,
}

fn eval(
    rules: &Rules,
    format: Format,
    show: &[String],
    examples: usize,
    pairs: Option<&PathBuf>,
    source: &PathBuf,
    expected: &PathBuf,
) -> Result<(), Box<dyn Error>> {
    let links: HashMap<String, String> = match pairs {
        Some(path) => std::fs::read_to_string(path)?
            .lines()
            .filter_map(|line| line.split_once(char::is_whitespace))
            .map(|(a, b)| (a.to_string(), b.trim().to_string()))
            .collect(),
        None => HashMap::new(),
    };
    let read = |path: &PathBuf| -> Result<Vec<Record>, Box<dyn Error>> {
        let reader = Reader::new(format, BufReader::new(File::open(path)?));
        Ok(reader.filter_map(Result::ok).collect())
    };
    let id = |record: &Record| record.field(1).and_then(|field| match field {
        Field::Control(_, value) => Some(value.clone()),
        _ => None,
    });
    let expected: HashMap<String, Record> = read(expected)?.into_iter()
        .filter_map(|record| id(&record).map(|id| (id, record)))
        .collect();
    let mut stats: BTreeMap<u16, TagStats> = BTreeMap::new();
    let (mut pairs, mut identical) = (0, 0);
    for record in read(source)? {
        let key = |id: String| if links.is_empty() { Some(id) } else { links.get(&id).cloned() };
        let Some((id, expected)) = id(&record).and_then(|id| key(id.clone()).and_then(|k| expected.get(&k)).map(|e| (id, e))) else {
            continue;
        };
        pairs += 1;
        let produced = rules.apply(&record);
        let (expected, produced) = (by_tag(expected), by_tag(&produced));
        if expected == produced {
            identical += 1;
        }
        // Each tag once, whether expected, produced or both
        for tag in expected.keys().chain(produced.keys().filter(|t| !expected.contains_key(t))) {
            let stat = stats.entry(*tag).or_default();
            let (e, p) = (expected.get(tag), produced.get(tag));
            stat.expected += e.is_some() as usize;
            stat.produced += p.is_some() as usize;
            if e == p {
                stat.equal += 1;
            } else {
                stat.differences.push((id.clone(), e.cloned().unwrap_or_default(), p.cloned().unwrap_or_default()));
            }
        }
    }

    if let Some(name) = rules.name() {
        println!("{name} {}", rules.version().unwrap_or(""));
    }
    println!("{pairs} pairs, {identical} identical records ({:.1}%)\n", percent(identical, pairs));
    println!("tag  expected produced    equal      %");
    let mut tags: Vec<(&u16, &TagStats)> = stats.iter().collect();
    tags.sort_by_key(|(tag, stat)| (std::cmp::Reverse(stat.expected.max(stat.produced)), **tag));
    for (tag, stat) in &tags {
        let union = stat.equal + stat.differences.len();
        println!("{tag:03} {:9} {:8} {:8} {:6.1}", stat.expected, stat.produced, stat.equal, percent(stat.equal, union));
    }
    for tag in show {
        let Some(stat) = tag.parse::<u16>().ok().and_then(|tag| stats.get(&tag)) else {
            println!("\n{tag}: no such tag");
            continue;
        };
        println!("\n=== {tag}: {} differences", stat.differences.len());
        for (id, expected, produced) in stat.differences.iter().take(examples) {
            println!("--- {id}");
            expected.iter().for_each(|line| println!("  - {line}"));
            produced.iter().for_each(|line| println!("  + {line}"));
        }
    }
    Ok(())
}

fn percent(n: usize, total: usize) -> f64 {
    if total == 0 { 100.0 } else { 100.0 * n as f64 / total as f64 }
}

/// The fields of `record` as MARCMaker-like lines, grouped by tag.
fn by_tag(record: &Record) -> BTreeMap<u16, Vec<String>> {
    let mut tags: BTreeMap<u16, Vec<String>> = BTreeMap::new();
    for field in &record.fields {
        let line = match field {
            Field::Control(_, value) => value.replace(' ', "\\"),
            Field::Standard(_, ind, subfields) => {
                let mut line: String = ind.iter().map(|c| if *c == ' ' { '\\' } else { *c }).collect();
                for Subfield(code, value) in subfields {
                    line.push('$');
                    line.push(*code);
                    line.push_str(value);
                }
                line
            }
        };
        tags.entry(*field.tag()).or_default().push(line);
    }
    tags
}
