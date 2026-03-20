use anyhow::Result;
use std::collections::HashMap;

fn u32_at(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

pub struct ParagraphStyle {
    pub fc_start: u32,
    pub fc_end: u32,
    pub istd: u16,
}

/// Parse PlcBtePapx to get paragraph FC ranges and their style indices.
pub fn parse_papx(
    table: &[u8],
    wd: &[u8],
    fc_plcf: u32,
    lcb_plcf: u32,
) -> Result<Vec<ParagraphStyle>> {
    let start = fc_plcf as usize;
    let end = start + lcb_plcf as usize;
    if end > table.len() {
        anyhow::bail!("PlcfBtePapx extends beyond table stream");
    }
    let plcf = &table[start..end];

    // PlcBtePapx: (n+1) FCs (4 bytes) + n PnBtePapx (4 bytes each)
    // Total = (n+1)*4 + n*4 = 4 + n*8
    let n = (lcb_plcf as usize - 4) / 8;

    let mut results = Vec::new();

    for i in 0..n {
        let pn_off = (n + 1) * 4 + i * 4;
        let pn = u32_at(plcf, pn_off);

        // Each PapxFkp page is 512 bytes in WordDocument at offset pn * 512
        let page_off = (pn as usize) * 512;
        if page_off + 512 > wd.len() {
            continue;
        }
        let page = &wd[page_off..page_off + 512];

        // Last byte = cpara (number of paragraph runs)
        let cpara = page[511] as usize;

        // (cpara+1) FCs at start, then cpara BX entries (13 bytes each)
        // BX: 1 byte offset (in words) into page for PapxInFkp
        let fc_array_size = (cpara + 1) * 4;

        for j in 0..cpara {
            let fc_start = u32_at(page, j * 4);
            let fc_end = u32_at(page, (j + 1) * 4);

            let bx_off = fc_array_size + j * 13;
            if bx_off >= 511 {
                continue;
            }
            let papx_word_off = page[bx_off] as usize;
            if papx_word_off == 0 {
                continue;
            }
            let papx_byte_off = papx_word_off * 2;
            if papx_byte_off >= 511 {
                continue;
            }

            // PapxInFkp: first byte = cb
            let cb = page[papx_byte_off] as usize;
            let grpprl_start;
            let grpprl_len;

            if cb == 0 {
                // cb == 0: next byte is cb', actual data starts after that
                if papx_byte_off + 1 >= 511 {
                    continue;
                }
                let cb2 = page[papx_byte_off + 1] as usize * 2;
                grpprl_start = papx_byte_off + 2;
                grpprl_len = cb2;
            } else {
                grpprl_start = papx_byte_off + 1;
                grpprl_len = cb * 2 - 1;
            }

            // GrpPrlAndIstd: first 2 bytes = istd
            if grpprl_start + 2 > 512 {
                continue;
            }
            let istd = u16::from_le_bytes([page[grpprl_start], page[grpprl_start + 1]]);
            let _ = grpprl_len; // remaining bytes are sprms, not needed for heading detection

            results.push(ParagraphStyle {
                fc_start,
                fc_end,
                istd,
            });
        }
    }

    results.sort_by_key(|p| p.fc_start);
    Ok(results)
}

/// Map character positions to heading levels using paragraph styles and piece table.
/// Returns a map from CP of paragraph start to heading level.
pub fn map_cp_to_heading(
    pieces: &[crate::clx::PieceDescriptor],
    para_styles: &[ParagraphStyle],
    heading_styles: &HashMap<u16, u8>,
) -> HashMap<u32, u8> {
    let mut cp_headings = HashMap::new();

    for ps in para_styles {
        let level = match heading_styles.get(&ps.istd) {
            Some(&l) => l,
            None => continue,
        };

        // Convert FC range to CP range using piece table
        for piece in pieces {
            let piece_fc_start;
            let bytes_per_char;

            if piece.compressed {
                piece_fc_start = piece.fc / 2;
                bytes_per_char = 1;
            } else {
                piece_fc_start = piece.fc;
                bytes_per_char = 2;
            }

            let piece_char_count = piece.cp_end - piece.cp_start;
            let piece_fc_end = piece_fc_start + piece_char_count * bytes_per_char;

            // Check if this paragraph's FC range overlaps with this piece
            let ps_fc_start = if piece.compressed {
                ps.fc_start / 2
            } else {
                ps.fc_start
            };

            if ps_fc_start >= piece_fc_start && ps_fc_start < piece_fc_end {
                let offset_in_piece = (ps_fc_start - piece_fc_start) / bytes_per_char;
                let cp = piece.cp_start + offset_in_piece;
                cp_headings.insert(cp, level);
            }
        }
    }

    cp_headings
}
