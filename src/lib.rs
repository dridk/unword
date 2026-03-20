pub mod clx;
pub mod fib;
pub mod markdown;
pub mod ole;
pub mod papx;
pub mod styles;
pub mod text;

use anyhow::Result;
use markdown::Document;

pub fn parse_doc(data: &[u8]) -> Result<Document> {
    let streams = ole::read_ole_streams(data)?;
    let fib = fib::parse_fib(&streams.word_document)?;

    let pieces = clx::parse_clx(&streams.table, fib.fc_clx, fib.lcb_clx)?;
    let chars = text::extract_text(&streams.word_document, &pieces)?;

    let heading_styles = styles::parse_stsh(&streams.table, fib.fc_stshf, fib.lcb_stshf)?;
    let para_styles = papx::parse_papx(
        &streams.table,
        &streams.word_document,
        fib.fc_plcf_bte_papx,
        fib.lcb_plcf_bte_papx,
    )?;
    let cp_headings = papx::map_cp_to_heading(&pieces, &para_styles, &heading_styles);

    let doc = markdown::generate_markdown(
        &chars,
        fib.ccp_text,
        fib.ccp_ftn,
        fib.ccp_hdd,
        fib.ccp_atn,
        fib.ccp_edn,
        fib.ccp_txbx,
        &cp_headings,
    );

    Ok(doc)
}
