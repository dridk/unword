use crate::clx::PieceDescriptor;
use anyhow::Result;

pub fn extract_text(wd: &[u8], pieces: &[PieceDescriptor]) -> Result<Vec<char>> {
    let mut chars = Vec::new();

    for piece in pieces {
        let char_count = (piece.cp_end - piece.cp_start) as usize;

        if piece.compressed {
            // ANSI: 1 byte per char, fc/2 is the byte offset
            let byte_off = (piece.fc / 2) as usize;
            let end = byte_off + char_count;
            if end > wd.len() {
                anyhow::bail!(
                    "Compressed piece extends beyond WordDocument: offset {byte_off}, count {char_count}, stream len {}",
                    wd.len()
                );
            }
            let bytes = &wd[byte_off..end];
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            chars.extend(decoded.chars());
        } else {
            // UTF-16LE: 2 bytes per char
            let byte_off = piece.fc as usize;
            let end = byte_off + char_count * 2;
            if end > wd.len() {
                anyhow::bail!(
                    "Uncompressed piece extends beyond WordDocument: offset {byte_off}, count {}, stream len {}",
                    char_count * 2,
                    wd.len()
                );
            }
            for i in 0..char_count {
                let off = byte_off + i * 2;
                let code_unit = u16::from_le_bytes([wd[off], wd[off + 1]]);
                if let Some(c) = char::from_u32(code_unit as u32) {
                    chars.push(c);
                } else {
                    chars.push('\u{FFFD}');
                }
            }
        }
    }

    Ok(chars)
}
