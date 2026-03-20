use anyhow::Result;
use std::collections::HashMap;

fn u16_at(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}

/// Returns a map from istd (style index) to heading level (1-9), or None for non-heading styles.
pub fn parse_stsh(table: &[u8], fc_stshf: u32, lcb_stshf: u32) -> Result<HashMap<u16, u8>> {
    let start = fc_stshf as usize;
    let end = start + lcb_stshf as usize;
    if end > table.len() {
        anyhow::bail!("STSH extends beyond table stream");
    }
    let stsh = &table[start..end];

    // LPStshi: 2 bytes cbStshi, then Stshi data
    if stsh.len() < 2 {
        anyhow::bail!("STSH too short");
    }
    let cb_stshi = u16_at(stsh, 0) as usize;
    let mut pos = 2 + cb_stshi;

    let mut map = HashMap::new();
    let mut istd: u16 = 0;

    while pos < stsh.len() {
        // LPStd: 2 bytes cbStd
        if pos + 2 > stsh.len() {
            break;
        }
        let cb_std = u16_at(stsh, pos) as usize;
        pos += 2;

        if cb_std == 0 {
            istd += 1;
            continue;
        }

        if pos + cb_std > stsh.len() {
            break;
        }

        let std_data = &stsh[pos..pos + cb_std];
        pos += cb_std;

        // StdfBase: first 10 bytes minimum
        if std_data.len() < 4 {
            istd += 1;
            continue;
        }

        // sti is in the first 2 bytes, bits 0-11
        let sti_raw = u16_at(std_data, 0);
        let sti = sti_raw & 0x0FFF;

        // sti 1-9 = Heading 1-9, sti 62 = Title (treat as Heading 1)
        if (1..=9).contains(&sti) {
            map.insert(istd, sti as u8);
        } else if sti == 62 {
            map.insert(istd, 1);
        }

        istd += 1;
    }

    Ok(map)
}
