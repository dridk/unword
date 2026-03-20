use anyhow::{Result, bail};

pub struct PieceDescriptor {
    pub cp_start: u32,
    pub cp_end: u32,
    pub fc: u32,
    pub compressed: bool,
}

fn u32_at(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

pub fn parse_clx(table: &[u8], fc_clx: u32, lcb_clx: u32) -> Result<Vec<PieceDescriptor>> {
    let start = fc_clx as usize;
    let end = start + lcb_clx as usize;
    if end > table.len() {
        bail!("CLX extends beyond table stream");
    }
    let clx = &table[start..end];

    // Skip RgPrc entries (type 0x01), find Pcdt (type 0x02)
    let mut pos = 0;
    while pos < clx.len() {
        let marker = clx[pos];
        if marker == 0x01 {
            // PrcData: skip cbGrpprl (2 bytes) + grpprl
            if pos + 3 > clx.len() {
                bail!("Truncated PrcData");
            }
            let cb = u16::from_le_bytes([clx[pos + 1], clx[pos + 2]]) as usize;
            pos += 3 + cb;
        } else if marker == 0x02 {
            pos += 1;
            break;
        } else {
            bail!("Unexpected CLX marker: {:#04X}", marker);
        }
    }

    // Pcdt: 4 bytes lcb, then PlcPcd
    if pos + 4 > clx.len() {
        bail!("Truncated Pcdt header");
    }
    let lcb = u32::from_le_bytes([clx[pos], clx[pos + 1], clx[pos + 2], clx[pos + 3]]) as usize;
    pos += 4;

    let plc_pcd = &clx[pos..pos + lcb];

    // PlcPcd: (n+1) CPs (4 bytes each) followed by n PCDs (8 bytes each)
    // Total size = (n+1)*4 + n*8 = 4 + n*12
    // So n = (lcb - 4) / 12
    let n = (lcb - 4) / 12;
    let cp_array_end = (n + 1) * 4;

    let mut pieces = Vec::with_capacity(n);
    for i in 0..n {
        let cp_start = u32_at(plc_pcd, i * 4);
        let cp_end = u32_at(plc_pcd, (i + 1) * 4);

        let pcd_off = cp_array_end + i * 8;
        // PCD: 2 bytes flags, 4 bytes fc_compressed, 2 bytes prm
        let fc_raw = u32_at(plc_pcd, pcd_off + 2);
        let compressed = (fc_raw & (1 << 30)) != 0;
        let fc = fc_raw & 0x3FFF_FFFF;

        pieces.push(PieceDescriptor {
            cp_start,
            cp_end,
            fc,
            compressed,
        });
    }

    Ok(pieces)
}
