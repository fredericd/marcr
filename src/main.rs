use std::error::Error;
use std::fs::File;
use marcr::{ Record, Field, Subfield, Format, Writer };
use std::io::{BufReader, BufWriter};


fn read_iso2709() -> Result<(), Box<dyn Error>> {
    // let file = File::open("/home/tamil/bnf-2022.mrc")?;
    let file = File::open("/home/tamil/backup/demo/dump.mrc")?;
    let buf_reader = BufReader::with_capacity(8192 * 100, file);
    let format = Format::Iso2709;
    let mut reader = marcr::Reader::new(format, buf_reader);
    let file_name = "file.txt";
    let output_file = File::create(file_name)?;
    let buf_writer = BufWriter::new(output_file);
    let mut writer = Writer::new(Format::Text, buf_writer);
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
            Err(e) => eprintln!("{e}"),
        };
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    read_iso2709()
}

