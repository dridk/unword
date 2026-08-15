# unword

Convert legacy Microsoft Word `.doc` files (OLE/CFB format) to Markdown. Inspired by [antiword](http://www.winfield.demon.nl/).

Extracts body text with heading levels, page breaks, and textbox contents. No external dependencies (no LibreOffice, no COM).

## Installation

### CLI (Rust)

```bash
cargo install unword
```

### Python

```
pip install unword
```

### From source 

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

### Inspecting a file that fails to parse

```bash
unword -i document.doc --inspect
```

Prints the OLE2 stream layout and the FIB header fields, and nothing else — no
document text — so the output can be attached to a bug report:

```
Streams:
  /WordDocument (5181 bytes)
  /1Table (3788 bytes)
  /SummaryInformation (172 bytes)
FIB: wIdent=0xA5EC nFib=257 (Word 2002)
     flags=0x12F0 fComplex=false fEncrypted=false fWhichTblStm=1 (expects 1Table)
```

## Supported files

- Word 97 and later (`nFib` >= 193). Word 6.0/95 files store their tables
  inside the `WordDocument` stream and are not supported; convert them first
  (`libreoffice --convert-to doc file.doc`) or use antiword.
- Encrypted or password-protected documents are not supported.
- Containers whose directory tree is unsorted or damaged are read with a
  built-in fallback reader, the same way LibreOffice and wvWare handle them.

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


## Alternative 

- antiword
- abiword
- tika
- libreoffice
