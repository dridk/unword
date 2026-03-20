# unword

Convert legacy Microsoft Word `.doc` files (OLE/CFB format) to Markdown. Inspired by [antiword](http://www.winfield.demon.nl/).

Extracts body text with heading levels, page breaks, and textbox contents. No external dependencies (no LibreOffice, no COM).

## Installation

### CLI (Rust)

```bash
cargo build --release
```

### Python

Requires [maturin](https://www.maturin.rs/) and a virtual environment:

```bash
uv venv .venv && source .venv/bin/activate
maturin develop
```

Or build a wheel:

```bash
maturin build --release
pip install target/wheels/unword-*.whl
```

## Usage

### CLI

```bash
# Print to stdout
unword -i document.doc

# Write to file
unword -i document.doc -o output.md
```

### Python

```python
import unword

doc = unword.parse_doc(open("document.doc", "rb").read())

print(doc.body_text)      # Markdown string with headings
print(doc.textboxes)      # List of textbox strings
```

### Rust library

```rust
let data = std::fs::read("document.doc")?;
let doc = unword::parse_doc(&data)?;
println!("{}", doc.body_text);
```

## Output format

- Headings are rendered as `#`, `##`, `###`, etc. based on Word styles
- Paragraphs are separated by blank lines
- Page breaks become `---`
- Textboxes are extracted separately

## Tests

```bash
# Rust
cargo test

# Python
pytest tests/test_python.py
```

## License

MIT
