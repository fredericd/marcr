use std::error::Error;
use std::fs::File;
use marcr::{ Record, Field, Subfield, Iso2709Reader };
use std::io::{BufWriter, BufReader};


fn main() -> Result<(), Box<dyn Error>> {
    let file = File::open("/home/tamil/backup/demo/dump.mrc")?;
    let buf_reader = BufReader::with_capacity(8192 * 100, file);
    let mut reader = Iso2709Reader::new(buf_reader);

    while let Some(record) = reader.read()? {
        println!("{record}\n");
    }
    Ok(())
}

