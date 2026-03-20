use std::collections::HashMap;

use crate::fields::{self, Field};

pub struct Document {
    pub body_text: String,
    pub textboxes: Vec<String>,
    pub fields: Vec<Field>,
}

pub fn generate_markdown(
    chars: &[char],
    ccp_text: u32,
    ccp_ftn: u32,
    ccp_hdd: u32,
    ccp_atn: u32,
    ccp_edn: u32,
    ccp_txbx: u32,
    cp_headings: &HashMap<u32, u8>,
    strip_fields: bool,
) -> Document {
    let total = chars.len() as u32;
    let body_end = ccp_text.min(total);
    let body_chars = &chars[..body_end as usize];

    // Extract fields from body text
    let extracted_fields = fields::extract_fields(body_chars);

    // Extract body paragraphs
    let body_text = render_paragraphs(body_chars, 0, cp_headings, strip_fields);

    // Extract textboxes
    let txbx_start = ccp_text + ccp_ftn + ccp_hdd + ccp_atn + ccp_edn;
    let txbx_end = (txbx_start + ccp_txbx).min(total);

    let mut textboxes = Vec::new();
    if txbx_start < txbx_end {
        let txbx_chars = &chars[txbx_start as usize..txbx_end as usize];
        let processed: Vec<char> = if strip_fields {
            fields::strip_field_codes(txbx_chars).chars().collect()
        } else {
            txbx_chars.to_vec()
        };
        // Split textbox region on paragraph marks
        let mut current = String::new();
        for &c in &processed {
            if c == '\r' || c == '\x0D' {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    textboxes.push(trimmed);
                }
                current.clear();
            } else if !is_control(c) {
                current.push(c);
            }
        }
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            textboxes.push(trimmed);
        }
    }

    Document {
        body_text,
        textboxes,
        fields: extracted_fields,
    }
}

fn render_paragraphs(
    chars: &[char],
    cp_offset: u32,
    cp_headings: &HashMap<u32, u8>,
    strip_fields: bool,
) -> String {
    let mut output = String::new();
    let mut para_start: usize = 0;
    for (i, &c) in chars.iter().enumerate() {
        if c == '\r' || c == '\x0D' {
            let para_text = clean_text(&chars[para_start..i], strip_fields);
            if !para_text.is_empty() {
                if let Some(&level) = cp_headings.get(&(cp_offset + para_start as u32)) {
                    for _ in 0..level {
                        output.push('#');
                    }
                    output.push(' ');
                }
                output.push_str(&para_text);
                output.push_str("\n\n");
            }
            para_start = i + 1;
        } else if c == '\x0C' {
            // Page break
            let para_text = clean_text(&chars[para_start..i], strip_fields);
            if !para_text.is_empty() {
                output.push_str(&para_text);
                output.push_str("\n\n");
            }
            output.push_str("---\n\n");
            para_start = i + 1;
        }
    }

    // Handle last paragraph without trailing \r
    if para_start < chars.len() {
        let para_text = clean_text(&chars[para_start..], strip_fields);
        if !para_text.is_empty() {
            if let Some(&level) = cp_headings.get(&(cp_offset + para_start as u32)) {
                for _ in 0..level {
                    output.push('#');
                }
                output.push(' ');
            }
            output.push_str(&para_text);
            output.push_str("\n\n");
        }
    }

    output
}

fn clean_text(chars: &[char], strip_fields: bool) -> String {
    if strip_fields {
        let stripped = fields::strip_field_codes(chars);
        let mut s = String::with_capacity(stripped.len());
        for c in stripped.chars() {
            if !is_control(c) {
                s.push(c);
            }
        }
        s.trim().to_string()
    } else {
        let mut s = String::with_capacity(chars.len());
        for &c in chars {
            if is_control(c) {
                continue;
            }
            s.push(c);
        }
        s.trim().to_string()
    }
}

fn is_control(c: char) -> bool {
    matches!(c, '\x01' | '\x08' | '\x07')
}
