use anyhow::{Context, Result};
use clap::Parser;
use std::fs;

#[derive(Parser)]
#[command(name = "doc2text", about = "Convert MS-DOC (.doc) files to Markdown")]
struct Args {
    #[arg(short, long)]
    input: String,

    #[arg(short, long)]
    output: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let data = fs::read(&args.input).with_context(|| format!("Failed to read {}", args.input))?;
    let doc = doc2text::parse_doc(&data)?;

    let mut md = doc.body_text;

    if !doc.textboxes.is_empty() {
        md.push_str("---\n\n");
        for tb in &doc.textboxes {
            md.push_str(tb);
            md.push_str("\n\n");
        }
    }

    match args.output {
        Some(path) => {
            fs::write(&path, &md).with_context(|| format!("Failed to write {path}"))?;
            eprintln!("Written to {path}");
        }
        None => print!("{md}"),
    }

    Ok(())
}
