use anyhow::{Context, Result, bail};
use cfb::CompoundFile;
use std::io::{Cursor, Read};

pub struct OleStreams {
    pub word_document: Vec<u8>,
    pub table: Vec<u8>,
}

pub fn read_ole_streams(data: &[u8]) -> Result<OleStreams> {
    let cursor = Cursor::new(data);
    let mut cfb = CompoundFile::open(cursor).context("Failed to open OLE2 container")?;

    let word_document = read_stream(&mut cfb, "/WordDocument")?;

    // Determine which table stream (0Table or 1Table) based on FibBase.fWhichTblStm
    // Bit 9 of flags at offset 10 in WordDocument
    if word_document.len() < 12 {
        bail!("WordDocument stream too short");
    }
    let flags = u16::from_le_bytes([word_document[10], word_document[11]]);
    let which_table = (flags >> 9) & 1;
    let table_name = if which_table == 1 {
        "/1Table"
    } else {
        "/0Table"
    };

    let table = read_stream(&mut cfb, table_name)?;

    Ok(OleStreams {
        word_document,
        table,
    })
}

fn read_stream(cfb: &mut CompoundFile<Cursor<&[u8]>>, name: &str) -> Result<Vec<u8>> {
    let mut stream = cfb
        .open_stream(name)
        .with_context(|| format!("Failed to open stream {name}"))?;
    let mut buf = Vec::new();
    stream
        .read_to_end(&mut buf)
        .with_context(|| format!("Failed to read stream {name}"))?;
    Ok(buf)
}
