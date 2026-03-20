use std::fs;

const DOC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/1000.doc");

fn load_doc() -> doc2text::markdown::Document {
    let data = fs::read(DOC_PATH).expect("failed to read 1000.doc");
    doc2text::parse_doc(&data).expect("failed to parse doc")
}

#[test]
fn test_ole_streams() {
    let data = fs::read(DOC_PATH).expect("failed to read 1000.doc");
    let streams = doc2text::ole::read_ole_streams(&data).expect("failed to read OLE streams");
    assert_eq!(streams.word_document.len(), 5181);
    assert_eq!(streams.table.len(), 3788);
}

#[test]
fn test_fib_magic() {
    let data = fs::read(DOC_PATH).expect("failed to read 1000.doc");
    let streams = doc2text::ole::read_ole_streams(&data).unwrap();
    let fib = doc2text::fib::parse_fib(&streams.word_document).unwrap();
    assert_eq!(fib.ccp_text, 607);
    assert_eq!(fib.ccp_txbx, 51);
    assert_eq!(fib.ccp_ftn, 0);
}

#[test]
fn test_piece_table() {
    let data = fs::read(DOC_PATH).unwrap();
    let streams = doc2text::ole::read_ole_streams(&data).unwrap();
    let fib = doc2text::fib::parse_fib(&streams.word_document).unwrap();
    let pieces = doc2text::clx::parse_clx(&streams.table, fib.fc_clx, fib.lcb_clx).unwrap();
    assert!(!pieces.is_empty());

    let chars = doc2text::text::extract_text(&streams.word_document, &pieces).unwrap();
    let text: String = chars.iter().collect();
    assert!(text.contains("Concert du soir"));
    assert!(text.contains("chocolat"));
}

#[test]
fn test_heading_detection() {
    let doc = load_doc();
    assert!(doc.body_text.contains("# Concert du soir"));
    assert!(doc.body_text.contains("# Ceci est le titre"));
    assert!(doc.body_text.contains("## Sous titre"));
    assert!(doc.body_text.contains("### Sous sous titre"));
    assert!(doc.body_text.contains("#### Super sous titre"));
}

#[test]
fn test_body_text() {
    let doc = load_doc();
    assert!(doc.body_text.contains("Alors voila, je mange du chocolat"));
    assert!(doc.body_text.contains("et voila du texte. Avec une liste\u{a0}:"));
    assert!(doc.body_text.contains("- truc"));
    assert!(doc.body_text.contains("- truc muche"));
}

#[test]
fn test_textboxes() {
    let doc = load_doc();
    assert_eq!(doc.textboxes.len(), 3);
    assert_eq!(doc.textboxes[0], "ZONE DE TEXTE");
    assert_eq!(doc.textboxes[1], "ZONE DE TEXTE 2");
    assert_eq!(doc.textboxes[2], "ZONE DE TEXTE 3");
}

#[test]
fn test_no_control_chars_in_output() {
    let doc = load_doc();
    let full = format!("{}{}", doc.body_text, doc.textboxes.join(""));
    for c in full.chars() {
        assert!(
            !matches!(c, '\x01' | '\x07' | '\x08' | '\x13' | '\x14' | '\x15'),
            "control char {:#04x} found in output",
            c as u32
        );
    }
}

#[test]
fn test_invalid_file() {
    let result = doc2text::parse_doc(b"not a doc file");
    assert!(result.is_err());
}
