use anyhow::{Result, bail};

pub struct Fib {
    pub ccp_text: u32,
    pub ccp_ftn: u32,
    pub ccp_hdd: u32,
    pub ccp_atn: u32,
    pub ccp_edn: u32,
    pub ccp_txbx: u32,
    pub fc_clx: u32,
    pub lcb_clx: u32,
    pub fc_stshf: u32,
    pub lcb_stshf: u32,
    pub fc_plcf_bte_papx: u32,
    pub lcb_plcf_bte_papx: u32,
}

fn u16_at(data: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([data[off], data[off + 1]])
}

fn u32_at(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

pub fn parse_fib(wd: &[u8]) -> Result<Fib> {
    if wd.len() < 898 {
        bail!("WordDocument stream too short for FIB");
    }

    let w_ident = u16_at(wd, 0);
    if w_ident != 0xA5EC {
        bail!("Invalid wIdent: expected 0xA5EC, got {:#06X}", w_ident);
    }

    // FibBase is 32 bytes at offset 0
    // csw (count of shorts) at offset 32
    // FibRgW97 starts at offset 34, length = csw * 2
    let csw = u16_at(wd, 32) as usize;
    let rg_w_end = 34 + csw * 2;

    // cslw (count of longs) at rg_w_end
    let cslw_off = rg_w_end;
    let _cslw = u16_at(wd, cslw_off) as usize;
    let rg_lw_start = cslw_off + 2;

    // FibRgLw97: ccpText at offset 3*4=12 from rg_lw_start
    let ccp_text = u32_at(wd, rg_lw_start + 12);
    let ccp_ftn = u32_at(wd, rg_lw_start + 16);
    let ccp_hdd = u32_at(wd, rg_lw_start + 20);
    // offset 24 = ccpMcr (skip)
    let ccp_atn = u32_at(wd, rg_lw_start + 28);
    let ccp_edn = u32_at(wd, rg_lw_start + 32);
    let ccp_txbx = u32_at(wd, rg_lw_start + 36);

    // cbRgFcLcb at end of FibRgLw97
    let rg_lw_end = rg_lw_start + _cslw * 4;
    let _cb_rg_fc_lcb = u16_at(wd, rg_lw_end);
    let rg_fc_start = rg_lw_end + 2;

    // FibRgFcLcb97 offsets (each pair is 8 bytes: fc + lcb)
    // fcStshf/lcbStshf: pair index 1 → byte offset 8
    let fc_stshf = u32_at(wd, rg_fc_start + 8);
    let lcb_stshf = u32_at(wd, rg_fc_start + 12);

    // fcPlcfBtePapx/lcbPlcfBtePapx: pair index 13 → byte offset 104
    let fc_plcf_bte_papx = u32_at(wd, rg_fc_start + 104);
    let lcb_plcf_bte_papx = u32_at(wd, rg_fc_start + 108);

    // fcClx/lcbClx: pair index 33 → byte offset 264
    let fc_clx = u32_at(wd, rg_fc_start + 264);
    let lcb_clx = u32_at(wd, rg_fc_start + 268);

    Ok(Fib {
        ccp_text,
        ccp_ftn,
        ccp_hdd,
        ccp_atn,
        ccp_edn,
        ccp_txbx,
        fc_clx,
        lcb_clx,
        fc_stshf,
        lcb_stshf,
        fc_plcf_bte_papx,
        lcb_plcf_bte_papx,
    })
}
