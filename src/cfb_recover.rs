//! Minimal, tolerant read-only OLE2/CFB reader, used as a fallback.
//!
//! The `cfb` crate resolves a stream name by descending the red-black tree
//! formed by the sibling pointers of the directory entries. Some `.doc`
//! writers emit directory trees whose sibling ordering does not follow the
//! MS-CFB collation rules: every entry is still reachable, but a binary
//! search walks the wrong way and misses it. That is why a file can open in
//! LibreOffice or wvWare (both scan the directory linearly) while `cfb`
//! reports `Failed to open stream /0Table` for a stream that is present.
//!
//! This module reads the FAT / mini-FAT by hand and looks entries up with a
//! linear scan, ignoring the tree structure entirely. It is only used when
//! the normal path fails.

use anyhow::{Result, bail};

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
/// Highest sector number that refers to real data (above this are the
/// DIFSECT/FATSECT/ENDOFCHAIN/FREESECT markers).
const MAX_REG_SECT: u32 = 0xFFFF_FFFA;
const OBJ_STREAM: u8 = 2;
const OBJ_ROOT: u8 = 5;
const DIR_ENTRY_LEN: usize = 128;

fn u16_at(data: &[u8], off: usize) -> u16 {
    match data.get(off..off + 2) {
        Some(b) => u16::from_le_bytes([b[0], b[1]]),
        None => 0,
    }
}

fn u32_at(data: &[u8], off: usize) -> u32 {
    match data.get(off..off + 4) {
        Some(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        None => 0,
    }
}

/// A directory entry, as stored in the container (no hierarchy).
pub struct RawEntry {
    pub name: String,
    pub obj_type: u8,
    pub size: u64,
    start_sector: u32,
}

impl RawEntry {
    pub fn is_stream(&self) -> bool {
        self.obj_type == OBJ_STREAM
    }
}

pub struct RawCfb<'a> {
    data: &'a [u8],
    sector_size: usize,
    mini_sector_size: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    entries: Vec<RawEntry>,
}

impl<'a> RawCfb<'a> {
    pub fn parse(data: &'a [u8]) -> Result<RawCfb<'a>> {
        if data.len() < 512 {
            bail!(
                "File is too small to be an OLE2 container ({} bytes)",
                data.len()
            );
        }
        if data[..8] != SIGNATURE {
            bail!("Not an OLE2/CFB container (bad signature)");
        }

        let sector_shift = u16_at(data, 30);
        if !(9..=24).contains(&sector_shift) {
            bail!("Unsupported OLE2 sector size (1 << {sector_shift})");
        }
        let sector_size = 1usize << sector_shift;

        let mini_shift = u16_at(data, 32);
        if !(2..=sector_shift).contains(&mini_shift) {
            bail!("Unsupported OLE2 mini sector size (1 << {mini_shift})");
        }
        let mini_sector_size = 1usize << mini_shift;

        let mini_cutoff = match u32_at(data, 56) {
            0 => 4096,
            n => n as u64,
        };
        let first_dir_sector = u32_at(data, 48);
        let first_mini_fat = u32_at(data, 60);
        let first_difat = u32_at(data, 68);

        let total_sectors = data.len() / sector_size;

        // --- FAT: the DIFAT lists the sectors holding the FAT itself ---
        let mut fat_sectors: Vec<u32> = Vec::new();
        for i in 0..109 {
            let id = u32_at(data, 76 + i * 4);
            if id <= MAX_REG_SECT {
                fat_sectors.push(id);
            }
        }
        let per_sector = sector_size / 4;
        let mut difat = first_difat;
        let mut visited = 0usize;
        while difat <= MAX_REG_SECT && visited <= total_sectors {
            visited += 1;
            let Some(off) = Self::sector_offset(data, sector_size, difat) else {
                break;
            };
            for i in 0..per_sector - 1 {
                let id = u32_at(data, off + i * 4);
                if id <= MAX_REG_SECT {
                    fat_sectors.push(id);
                }
            }
            difat = u32_at(data, off + (per_sector - 1) * 4);
        }

        let mut fat: Vec<u32> = Vec::with_capacity(fat_sectors.len() * per_sector);
        for id in fat_sectors {
            match Self::sector_offset(data, sector_size, id) {
                // Keep the FAT index aligned even when a sector is out of range.
                None => fat.resize(fat.len() + per_sector, u32::MAX),
                Some(off) => {
                    for i in 0..per_sector {
                        fat.push(u32_at(data, off + i * 4));
                    }
                }
            }
        }
        if fat.is_empty() {
            bail!("OLE2 container has no readable FAT");
        }

        let mut cfb = RawCfb {
            data,
            sector_size,
            mini_sector_size,
            mini_cutoff,
            fat,
            mini_fat: Vec::new(),
            mini_stream: Vec::new(),
            entries: Vec::new(),
        };

        // --- directory entries (linear scan, tree links ignored) ---
        let dir_bytes = cfb.read_fat_chain(first_dir_sector, u64::MAX);
        if dir_bytes.len() < DIR_ENTRY_LEN {
            bail!("OLE2 container has no readable directory");
        }
        // Version 3 containers use 32-bit stream sizes; the high dword of the
        // field is reserved and some writers leave garbage in it.
        let allow_64bit_size = u16_at(data, 26) >= 4;
        cfb.entries = dir_bytes
            .chunks_exact(DIR_ENTRY_LEN)
            .filter_map(|raw| parse_dir_entry(raw, allow_64bit_size))
            .collect();

        // --- mini FAT and mini stream (small streams live inside it) ---
        let mini_fat_bytes = cfb.read_fat_chain(first_mini_fat, u64::MAX);
        cfb.mini_fat = mini_fat_bytes
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();

        if let Some(root) = cfb.entries.iter().find(|e| e.obj_type == OBJ_ROOT) {
            let (start, size) = (root.start_sector, root.size);
            cfb.mini_stream = cfb.read_fat_chain(start, size);
        }

        Ok(cfb)
    }

    /// Streams present in the container, as `(name, size)` pairs.
    pub fn stream_list(&self) -> Vec<(String, u64)> {
        self.entries
            .iter()
            .filter(|e| e.is_stream())
            .map(|e| (e.name.clone(), e.size))
            .collect()
    }

    /// Read a stream by name, comparing names case-insensitively and ignoring
    /// where the entry sits in the directory tree.
    pub fn read_by_name(&self, name: &str) -> Option<Vec<u8>> {
        let entry = self
            .entries
            .iter()
            .find(|e| e.is_stream() && name_matches(&e.name, name))?;
        Some(self.read_entry(entry))
    }

    fn read_entry(&self, entry: &RawEntry) -> Vec<u8> {
        if entry.size < self.mini_cutoff {
            self.read_mini_chain(entry.start_sector, entry.size)
        } else {
            self.read_fat_chain(entry.start_sector, entry.size)
        }
    }

    fn sector_offset(data: &[u8], sector_size: usize, id: u32) -> Option<usize> {
        let off = (id as usize).checked_add(1)?.checked_mul(sector_size)?;
        if off.checked_add(sector_size)? <= data.len() {
            Some(off)
        } else {
            None
        }
    }

    fn chain(fat: &[u32], start: u32, max_sectors: usize) -> Vec<u32> {
        let mut sectors = Vec::new();
        let mut id = start;
        while id <= MAX_REG_SECT && sectors.len() < max_sectors && sectors.len() <= fat.len() {
            sectors.push(id);
            id = *fat.get(id as usize).unwrap_or(&u32::MAX);
        }
        sectors
    }

    fn read_fat_chain(&self, start: u32, size: u64) -> Vec<u8> {
        let max_sectors = sector_count(size, self.sector_size);
        let mut out = Vec::new();
        for id in Self::chain(&self.fat, start, max_sectors) {
            match Self::sector_offset(self.data, self.sector_size, id) {
                Some(off) => out.extend_from_slice(&self.data[off..off + self.sector_size]),
                None => break,
            }
        }
        truncate_to(out, size)
    }

    fn read_mini_chain(&self, start: u32, size: u64) -> Vec<u8> {
        let max_sectors = sector_count(size, self.mini_sector_size);
        let mut out = Vec::new();
        for id in Self::chain(&self.mini_fat, start, max_sectors) {
            let off = (id as usize) * self.mini_sector_size;
            match self.mini_stream.get(off..off + self.mini_sector_size) {
                Some(chunk) => out.extend_from_slice(chunk),
                None => break,
            }
        }
        truncate_to(out, size)
    }
}

fn sector_count(size: u64, sector_size: usize) -> usize {
    if size == u64::MAX {
        usize::MAX
    } else {
        size.div_ceil(sector_size as u64) as usize
    }
}

fn truncate_to(mut buf: Vec<u8>, size: u64) -> Vec<u8> {
    if size != u64::MAX && (size as usize) < buf.len() {
        buf.truncate(size as usize);
    }
    buf
}

fn parse_dir_entry(raw: &[u8], allow_64bit_size: bool) -> Option<RawEntry> {
    let obj_type = raw[66];
    if obj_type != OBJ_STREAM && obj_type != OBJ_ROOT {
        return None;
    }
    // cbName counts bytes, including the terminating NUL.
    let name_len = (u16_at(raw, 64) as usize).min(64);
    let units: Vec<u16> = raw[..name_len.saturating_sub(2)]
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let name = String::from_utf16_lossy(&units);
    let mut size = u32_at(raw, 120) as u64;
    if allow_64bit_size {
        size |= (u32_at(raw, 124) as u64) << 32;
    }
    Some(RawEntry {
        name,
        obj_type,
        size,
        start_sector: u32_at(raw, 116),
    })
}

/// Directory entry names are compared case-insensitively per MS-CFB; some
/// writers also leave stray whitespace or control characters around them.
pub fn name_matches(entry_name: &str, wanted: &str) -> bool {
    let clean = |s: &str| {
        s.trim_matches(|c: char| c.is_whitespace() || c.is_control())
            .to_string()
    };
    clean(entry_name).eq_ignore_ascii_case(&clean(wanted))
}
