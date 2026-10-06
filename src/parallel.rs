//! Parallel processing of a record stream (feature `parallel`).
//!
//! [`convert`] reads records with a [`Reader`], filters and transforms them
//! with a function run on all cores, and writes them with a [`Writer`], in
//! their original order. It works in three stages linked by bounded
//! queues, so that memory use stays bounded whatever the input size:
//!
//! 1. a thread cuts the input into batches of raw records;
//! 2. each batch is parsed, processed and serialized in parallel, on the
//!    current [rayon] thread pool (one thread per core by default);
//! 3. a thread writes the serialized records in order.

use std::io::{BufRead, Read, Write};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

use rayon::prelude::*;

use crate::{Error, Reader, Record, Writer};

/// Approximate size of a batch of raw records.
const BATCH_BYTES: usize = 4 * 1024 * 1024;
/// Batches waiting between two stages.
const QUEUE: usize = 4;

/// Counts returned by [`convert`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stats {
    /// Records extracted from the input, malformed ones included.
    pub read: usize,
    /// Records written to the output.
    pub written: usize,
    /// Records skipped because they could not be read or written.
    pub skipped: usize,
}

/// Raw records, concatenated: record `k` is `data[ends[k-1]..ends[k]]`.
struct Batch {
    data: Vec<u8>,
    ends: Vec<usize>,
    /// Number in the input stream and byte offset of each record.
    positions: Vec<(usize, u64)>,
}

impl Batch {
    fn record(&self, k: usize) -> &[u8] {
        let start = if k == 0 { 0 } else { self.ends[k - 1] };
        &self.data[start..self.ends[k]]
    }
}

/// Outcome of a record: serialized, filtered out (`None`), or an error.
type Outcome = Result<Option<Vec<u8>>, Error>;

/// Reads `reader`, applies `process` to each record on all cores, and
/// writes the records it returns to `writer`, in the input order.
///
/// - `process` filters and transforms a record: returning `None` drops it.
///   It runs on several threads at once, hence the `Fn + Sync` bound.
/// - `on_skip` is called, in input order, for each record that is skipped,
///   with its number in the input and the error: [`Error::Malformed`] when
///   it cannot be parsed, [`Error::Unwritable`] when it cannot be written
///   in the output format. Processing then goes on with the next record.
///
/// Returns the counts of records read, written and skipped, or the first
/// [`Error::Io`] met on input or output, which stops processing. The
/// output is the same, byte for byte, as with a sequential loop over
/// [`Reader::read`] and [`Writer::write`]. Call [`Writer::finish`]
/// afterwards, as usual.
///
/// Parallel work runs on the current rayon thread pool: one thread per
/// core by default, or the number set by the `RAYON_NUM_THREADS`
/// environment variable; call `convert` inside
/// `rayon::ThreadPool::install` to use a custom pool.
///
/// ```no_run
/// use marcr::{Format, Reader, Writer};
/// use std::fs::File;
/// use std::io::{BufReader, BufWriter};
///
/// # fn main() -> Result<(), marcr::Error> {
/// let reader = Reader::new(Format::Iso2709, BufReader::new(File::open("notices.mrc")?));
/// let mut writer = Writer::new(Format::Marcxml, BufWriter::new(File::create("notices.xml")?));
/// let stats = marcr::parallel::convert(
///     reader,
///     &mut writer,
///     |record| record.field(200).is_some().then_some(record), // keep records with a 200
///     |_number, error| eprintln!("skipped {error}"),
/// )?;
/// writer.finish()?;
/// eprintln!("{} read, {} written, {} skipped", stats.read, stats.written, stats.skipped);
/// # Ok(())
/// # }
/// ```
pub fn convert<R, W, F, S>(
    reader: Reader<R>,
    writer: &mut Writer<W>,
    process: F,
    on_skip: S,
) -> Result<Stats, Error>
where
    R: Read + BufRead + Send,
    W: Write + Send,
    F: Fn(Record) -> Option<Record> + Sync,
    S: FnMut(usize, Error) + Send,
{
    let input_format = reader.format;
    let output_format = writer.format;
    let (batch_tx, batch_rx) = sync_channel::<Batch>(QUEUE);
    let (done_tx, done_rx) = sync_channel::<(Batch, Vec<Outcome>)>(QUEUE);

    std::thread::scope(|scope| {
        let splitter = scope.spawn(move || split(reader, batch_tx));
        let output = scope.spawn(move || write(done_rx, writer, on_skip));

        // Stage 2, on the calling thread: parse, process and serialize
        // each batch in parallel; collect keeps the record order.
        for batch in batch_rx {
            let outcomes = (0..batch.ends.len())
                .into_par_iter()
                .map(|k| {
                    let (number, start) = batch.positions[k];
                    let record = input_format
                        .deserialize(batch.record(k))
                        .map_err(|e| e.at(number, start))?;
                    process(record).map(|record| output_format.serialize(&record)).transpose()
                })
                .collect();
            if done_tx.send((batch, outcomes)).is_err() {
                break; // the output stage stopped on an error
            }
        }
        drop(done_tx);

        let written = output.join().expect("marcr output thread panicked");
        let split = splitter.join().expect("marcr input thread panicked");
        let stats = written?;
        split?;
        Ok(stats)
    })
}

/// Stage 1: cuts the input into batches of raw records.
fn split<R: Read + BufRead>(mut reader: Reader<R>, tx: SyncSender<Batch>) -> Result<(), Error> {
    let format = reader.format;
    loop {
        let mut batch = Batch { data: Vec::with_capacity(BATCH_BYTES + BATCH_BYTES / 4), ends: Vec::new(), positions: Vec::new() };
        while batch.data.len() < BATCH_BYTES {
            let data = &mut batch.data;
            match reader.extract(format, |octets| {
                data.extend_from_slice(octets);
                data.len()
            })? {
                Some((end, number, start)) => {
                    batch.ends.push(end);
                    batch.positions.push((number, start));
                }
                None => break,
            }
        }
        if batch.ends.is_empty() {
            return Ok(()); // end of input
        }
        if tx.send(batch).is_err() {
            return Ok(()); // the next stages stopped
        }
    }
}

/// Stage 3: writes the serialized records in order and reports the
/// skipped ones.
fn write<W: Write, S: FnMut(usize, Error)>(
    rx: Receiver<(Batch, Vec<Outcome>)>,
    writer: &mut Writer<W>,
    mut on_skip: S,
) -> Result<Stats, Error> {
    let mut stats = Stats::default();
    for (batch, outcomes) in rx {
        for (k, outcome) in outcomes.into_iter().enumerate() {
            stats.read += 1;
            match outcome {
                Ok(Some(octets)) => {
                    writer.write_serialized(&octets)?;
                    stats.written += 1;
                }
                Ok(None) => {}
                Err(mut e) => {
                    // Number it in the output, as Writer::write does
                    if let Error::Unwritable { record, .. } = &mut e {
                        *record = Some(writer.count + 1);
                    }
                    stats.skipped += 1;
                    on_skip(batch.positions[k].0, e);
                }
            }
        }
    }
    Ok(stats)
}
