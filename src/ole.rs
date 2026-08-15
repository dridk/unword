use crate::cfb_recover::{RawCfb, name_matches};
use anyhow::{Result, bail};
use cfb::CompoundFile;
use std::fmt;
use std::io::{Cursor, Read};

#[derive(Debug)]
pub struct OleStreams {
    pub word_document: Vec<u8>,
    pub table: Vec<u8>,
}

const WORD_DOCUMENT: &str = "WordDocument";
const TABLE_0: &str = "0Table";
const TABLE_1: &str = "1Table";

/// The first bytes of the FIB, which tell us the Word version and where the
/// table stream lives.
#[derive(Clone, Copy)]
pub struct FibHeader {
    pub w_ident: u16,
    pub n_fib: u16,
    pub flags: u16,
}

impl FibHeader {
    fn parse(wd: &[u8]) -> Option<FibHeader> {
        if wd.len() < 12 {
            return None;
        }
        Some(FibHeader {
            w_ident: u16::from_le_bytes([wd[0], wd[1]]),
            n_fib: u16::from_le_bytes([wd[2], wd[3]]),
            flags: u16::from_le_bytes([wd[10], wd[11]]),
        })
    }

    pub fn f_complex(&self) -> bool {
        self.flags & (1 << 2) != 0
    }

    pub fn f_encrypted(&self) -> bool {
        self.flags & (1 << 8) != 0
    }

    /// `fWhichTblStm`: 1 → the document uses `1Table`, 0 → `0Table`.
    pub fn which_table(&self) -> u16 {
        (self.flags >> 9) & 1
    }

    pub fn table_name(&self) -> &'static str {
        if self.which_table() == 1 {
            TABLE_1
        } else {
            TABLE_0
        }
    }

    /// True for Word 6.0/95 files, which keep the tables inside the
    /// WordDocument stream instead of a separate table stream.
    pub fn is_word6(&self) -> bool {
        matches!(self.w_ident, 0xA5DB | 0xA5DC) || self.n_fib < 193
    }

    pub fn version_name(&self) -> &'static str {
        match self.n_fib {
            0..=100 => "Word 2.0 or earlier",
            101..=192 => "Word 6.0/95",
            193..=216 => "Word 97",
            217..=256 => "Word 2000",
            257..=267 => "Word 2002",
            268..=269 => "Word 2003",
            _ => "Word 2007 or later",
        }
    }
}

/// An OLE2 container, opened as leniently as possible.
struct Container<'a> {
    data: &'a [u8],
    cfb: Option<CompoundFile<Cursor<&'a [u8]>>>,
    raw: Option<RawCfb<'a>>,
    raw_tried: bool,
    /// Set when a stream could only be found with the fallback reader.
    used_recovery: bool,
}

impl<'a> Container<'a> {
    fn open(data: &'a [u8]) -> Result<Container<'a>> {
        let cfb = match CompoundFile::open(Cursor::new(data)) {
            Ok(cfb) => Some(cfb),
            Err(err) => {
                // The header may be damaged in ways the cfb crate rejects but
                // that other readers tolerate; only give up if we cannot make
                // sense of the container at all.
                if RawCfb::parse(data).is_err() {
                    return Err(anyhow::Error::new(err).context("Failed to open OLE2 container"));
                }
                None
            }
        };
        Ok(Container {
            data,
            cfb,
            raw: None,
            raw_tried: false,
            used_recovery: false,
        })
    }

    fn raw(&mut self) -> Option<&RawCfb<'a>> {
        if !self.raw_tried {
            self.raw_tried = true;
            self.raw = RawCfb::parse(self.data).ok();
        }
        self.raw.as_ref()
    }

    /// Read a top-level stream by name, falling back to a directory walk and
    /// then to the tolerant reader.
    fn read(&mut self, name: &str) -> Option<Vec<u8>> {
        if let Some(cfb) = self.cfb.as_mut() {
            if let Some(buf) = read_stream(cfb, &format!("/{name}")) {
                return Some(buf);
            }

            // The entry may sit in a sub-storage, or carry a slightly
            // different name. Prefer the shallowest match.
            let mut paths: Vec<String> = cfb
                .walk()
                .filter(|e| e.is_stream() && name_matches(e.name(), name))
                .map(|e| e.path().to_string_lossy().into_owned())
                .collect();
            paths.sort_by_key(|p| (p.matches('/').count(), p.len()));
            for path in paths {
                if let Some(buf) = read_stream(cfb, &path) {
                    return Some(buf);
                }
            }
        }

        // Unsorted or damaged directory tree: scan it linearly.
        let buf = self.raw()?.read_by_name(name)?;
        self.used_recovery = true;
        Some(buf)
    }

    fn stream_list(&mut self) -> Vec<(String, u64)> {
        if let Some(cfb) = self.cfb.as_ref() {
            let streams: Vec<(String, u64)> = cfb
                .walk()
                .filter(|e| e.is_stream())
                .map(|e| (e.path().to_string_lossy().into_owned(), e.len()))
                .collect();
            if !streams.is_empty() {
                return streams;
            }
        }
        self.raw().map(|raw| raw.stream_list()).unwrap_or_default()
    }
}

fn read_stream(cfb: &mut CompoundFile<Cursor<&[u8]>>, path: &str) -> Option<Vec<u8>> {
    let mut stream = cfb.open_stream(path).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn format_streams(streams: &[(String, u64)]) -> String {
    if streams.is_empty() {
        return "  (none)".to_string();
    }
    streams
        .iter()
        .map(|(name, size)| format!("  {name} ({size} bytes)"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn read_ole_streams(data: &[u8]) -> Result<OleStreams> {
    let mut container = Container::open(data)?;

    let word_document = match container.read(WORD_DOCUMENT) {
        Some(wd) => wd,
        None => bail!(
            "No WordDocument stream found — this OLE2 file does not look like a Word .doc \
             document.\nStreams in this container:\n{}",
            format_streams(&container.stream_list())
        ),
    };

    let Some(fib) = FibHeader::parse(&word_document) else {
        bail!(
            "WordDocument stream is too short ({} bytes)",
            word_document.len()
        );
    };

    if fib.f_encrypted() {
        bail!(
            "Document is encrypted or password-protected (fEncrypted is set); unword cannot read it"
        );
    }

    // fWhichTblStm says which table stream to use, but some writers set it
    // wrongly, so try the other one before giving up.
    let preferred = fib.table_name();
    let fallback = if preferred == TABLE_1 {
        TABLE_0
    } else {
        TABLE_1
    };
    let table = match container
        .read(preferred)
        .or_else(|| container.read(fallback))
    {
        Some(table) => table,
        None if fib.is_word6() => bail!(
            "This is a Word 6.0/95 document (wIdent={:#06X}, nFib={}), which stores its tables \
             inside the WordDocument stream. unword only supports Word 97 and later; convert the \
             file first (e.g. `libreoffice --convert-to doc`) or use antiword.",
            fib.w_ident,
            fib.n_fib
        ),
        None => bail!(
            "Neither the {preferred} nor the {fallback} stream could be found (the FIB asks for \
             {preferred}).\nStreams in this container:\n{}\nPlease report this file at \
             https://github.com/dridk/unword/issues (`unword -i FILE --inspect` prints this \
             listing without any document content).",
            format_streams(&container.stream_list())
        ),
    };

    Ok(OleStreams {
        word_document,
        table,
    })
}

/// Diagnostic view of a `.doc` file: container layout and FIB header, with no
/// document content, so it is safe to paste into a bug report.
pub struct Inspection {
    pub streams: Vec<(String, u64)>,
    pub fib: Option<FibHeader>,
    pub notes: Vec<String>,
}

pub fn inspect(data: &[u8]) -> Result<Inspection> {
    let mut container = Container::open(data)?;
    let mut notes = Vec::new();

    if container.cfb.is_none() {
        notes.push(
            "the container header is damaged; it was read with the fallback reader".to_string(),
        );
    }

    let streams = container.stream_list();
    let word_document = container.read(WORD_DOCUMENT);

    let fib = word_document.as_deref().and_then(FibHeader::parse);
    if let Some(fib) = fib {
        if fib.f_encrypted() {
            notes.push("the document is encrypted (fEncrypted)".to_string());
        }
        if fib.is_word6() {
            notes.push("Word 6.0/95 format: unword only supports Word 97 and later".to_string());
        } else if container.read(fib.table_name()).is_none() {
            notes.push(format!(
                "the {} stream requested by the FIB is missing",
                fib.table_name()
            ));
        }
    } else if word_document.is_none() {
        notes.push("no WordDocument stream in this container".to_string());
    }

    if container.used_recovery {
        notes.push(
            "the directory tree is not sorted as MS-CFB requires; streams were located by \
             scanning it linearly"
                .to_string(),
        );
    }

    Ok(Inspection {
        streams,
        fib,
        notes,
    })
}

impl fmt::Display for Inspection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Streams:")?;
        writeln!(f, "{}", format_streams(&self.streams))?;
        match self.fib {
            Some(fib) => {
                writeln!(
                    f,
                    "FIB: wIdent={:#06X} nFib={} ({})",
                    fib.w_ident,
                    fib.n_fib,
                    fib.version_name()
                )?;
                writeln!(
                    f,
                    "     flags={:#06X} fComplex={} fEncrypted={} fWhichTblStm={} (expects {})",
                    fib.flags,
                    fib.f_complex(),
                    fib.f_encrypted(),
                    fib.which_table(),
                    fib.table_name()
                )?;
            }
            None => writeln!(f, "FIB: unreadable")?,
        }
        for note in &self.notes {
            writeln!(f, "note: {note}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cfb::CompoundFile;
    use std::io::Write;

    /// Build a minimal OLE2 file containing the given streams.
    fn build_doc(streams: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut cfb = CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        for (name, content) in streams {
            let mut stream = cfb.create_stream(format!("/{name}")).unwrap();
            stream.write_all(content).unwrap();
            stream.flush().unwrap();
        }
        cfb.into_inner().into_inner()
    }

    /// A WordDocument stream long enough to hold a FIB header.
    fn word_document(w_ident: u16, n_fib: u16, flags: u16) -> Vec<u8> {
        let mut wd = vec![0u8; 1024];
        wd[0..2].copy_from_slice(&w_ident.to_le_bytes());
        wd[2..4].copy_from_slice(&n_fib.to_le_bytes());
        wd[10..12].copy_from_slice(&flags.to_le_bytes());
        wd
    }

    fn word97(flags: u16) -> Vec<u8> {
        word_document(0xA5EC, 193, flags)
    }

    #[test]
    fn reads_the_table_stream_named_by_the_fib() {
        let data = build_doc(&[
            ("WordDocument", word97(1 << 9)),
            ("1Table", b"one".to_vec()),
            ("0Table", b"zero".to_vec()),
        ]);
        let streams = read_ole_streams(&data).unwrap();
        assert_eq!(streams.table, b"one");
        assert_eq!(streams.word_document.len(), 1024);
    }

    #[test]
    fn falls_back_to_the_other_table_stream() {
        // fWhichTblStm asks for 0Table, but the file only has 1Table.
        let data = build_doc(&[("WordDocument", word97(0)), ("1Table", b"one".to_vec())]);
        let streams = read_ole_streams(&data).unwrap();
        assert_eq!(streams.table, b"one");
    }

    /// Swap the sibling pointers of a directory entry so that the red-black
    /// descent used by `cfb` walks the wrong way, as happens with files
    /// written by non-Microsoft tools. Every entry stays reachable by a
    /// linear scan.
    fn break_directory_sort(data: &mut [u8], entry_names: &[&str]) {
        const NO_STREAM: u32 = 0xFFFF_FFFF;
        for name in entry_names {
            let needle: Vec<u8> = name.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
            let start = data
                .windows(needle.len())
                .position(|w| w == needle)
                .expect("directory entry not found");
            let left = u32::from_le_bytes(data[start + 68..start + 72].try_into().unwrap());
            let right = u32::from_le_bytes(data[start + 72..start + 76].try_into().unwrap());
            if left == NO_STREAM && right == NO_STREAM {
                continue; // leaf: swapping would change nothing
            }
            data[start + 68..start + 72].copy_from_slice(&right.to_le_bytes());
            data[start + 72..start + 76].copy_from_slice(&left.to_le_bytes());
            return;
        }
        panic!("no directory entry with siblings to swap");
    }

    #[test]
    fn recovers_streams_from_an_unsorted_directory() {
        let mut data = build_doc(&[
            ("WordDocument", word97(1 << 9)),
            ("1Table", b"one".to_vec()),
        ]);
        break_directory_sort(&mut data, &["WordDocument", "1Table"]);

        // The cfb crate rejects the container outright...
        assert!(
            CompoundFile::open(Cursor::new(&data[..])).is_err(),
            "the test file is not actually broken"
        );

        // ...but we still read the document.
        let streams = read_ole_streams(&data).unwrap();
        assert_eq!(streams.table, b"one");
        assert_eq!(streams.word_document.len(), 1024);

        let report = inspect(&data).unwrap();
        assert!(report.notes.iter().any(|n| n.contains("not sorted")));
    }

    #[test]
    fn recovers_a_large_stream_from_an_unsorted_directory() {
        // Streams above the mini-stream cutoff live in the main FAT.
        let big = vec![0x42u8; 10_000];
        let mut data = build_doc(&[("WordDocument", word97(1 << 9)), ("1Table", big.clone())]);
        break_directory_sort(&mut data, &["WordDocument", "1Table"]);
        let streams = read_ole_streams(&data).unwrap();
        assert_eq!(streams.table, big);
    }

    #[test]
    fn reports_word6_documents_clearly() {
        let data = build_doc(&[("WordDocument", word_document(0xA5DC, 104, 0))]);
        let err = read_ole_streams(&data).unwrap_err().to_string();
        assert!(err.contains("Word 6.0/95"), "{err}");
    }

    #[test]
    fn reports_a_missing_table_stream_with_the_stream_listing() {
        let data = build_doc(&[("WordDocument", word97(1 << 9)), ("Data", b"x".to_vec())]);
        let err = read_ole_streams(&data).unwrap_err().to_string();
        assert!(err.contains("Neither the 1Table nor the 0Table"), "{err}");
        assert!(err.contains("/Data"), "{err}");
    }

    #[test]
    fn reports_encrypted_documents() {
        let data = build_doc(&[
            ("WordDocument", word97(1 << 8 | 1 << 9)),
            ("1Table", b"one".to_vec()),
        ]);
        let err = read_ole_streams(&data).unwrap_err().to_string();
        assert!(err.contains("encrypted"), "{err}");
    }

    #[test]
    fn reports_a_container_without_a_word_document_stream() {
        let data = build_doc(&[("Book", b"xls?".to_vec())]);
        let err = read_ole_streams(&data).unwrap_err().to_string();
        assert!(err.contains("No WordDocument stream"), "{err}");
        assert!(err.contains("/Book"), "{err}");
    }

    #[test]
    fn finds_a_word_document_stream_inside_a_storage() {
        let mut cfb = CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        cfb.create_storage("/ObjectPool").unwrap();
        for (name, content) in [
            ("WordDocument", word97(1 << 9)),
            ("1Table", b"one".to_vec()),
        ] {
            let mut stream = cfb.create_stream(format!("/ObjectPool/{name}")).unwrap();
            stream.write_all(&content).unwrap();
            stream.flush().unwrap();
        }
        let data = cfb.into_inner().into_inner();

        let streams = read_ole_streams(&data).unwrap();
        assert_eq!(streams.table, b"one");
    }

    #[test]
    fn inspection_lists_streams_and_fib_fields() {
        let data = build_doc(&[
            ("WordDocument", word97(1 << 9)),
            ("1Table", b"one".to_vec()),
        ]);
        let report = inspect(&data).unwrap().to_string();
        assert!(report.contains("/WordDocument (1024 bytes)"), "{report}");
        assert!(report.contains("nFib=193"), "{report}");
        assert!(
            report.contains("fWhichTblStm=1 (expects 1Table)"),
            "{report}"
        );
    }
}
