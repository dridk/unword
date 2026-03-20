/// Word field markers
const FIELD_BEGIN: char = '\x13';
const FIELD_SEP: char = '\x14';
const FIELD_END: char = '\x15';

#[derive(Debug, Clone)]
pub struct Field {
    pub field_type: String,
    pub name: String,
    pub value: String,
}

/// Extract all fields from the character stream.
/// Handles nested fields by using a stack.
pub fn extract_fields(chars: &[char]) -> Vec<Field> {
    let mut fields = Vec::new();
    let mut stack: Vec<(String, String)> = Vec::new(); // (code, value)
    let mut in_result: Vec<bool> = Vec::new(); // whether we've seen separator at each nesting level

    for &c in chars {
        match c {
            FIELD_BEGIN => {
                stack.push((String::new(), String::new()));
                in_result.push(false);
            }
            FIELD_SEP => {
                if let Some(last) = in_result.last_mut() {
                    *last = true;
                }
            }
            FIELD_END => {
                if let (Some((code, value)), Some(_)) = (stack.pop(), in_result.pop()) {
                    let (field_type, name) = parse_field_code(&code);
                    fields.push(Field {
                        field_type,
                        name,
                        value: value.trim().to_string(),
                    });
                }
            }
            _ => {
                if let Some(level) = stack.len().checked_sub(1) {
                    if in_result[level] {
                        stack[level].1.push(c);
                    } else {
                        stack[level].0.push(c);
                    }
                }
            }
        }
    }

    fields
}

/// Strip field codes from text, keeping only field results.
/// For fields without a separator (no result), nothing is kept.
pub fn strip_field_codes(chars: &[char]) -> String {
    let mut result = String::with_capacity(chars.len());
    let mut seen_separator: Vec<bool> = Vec::new();

    for &c in chars {
        match c {
            FIELD_BEGIN => {
                seen_separator.push(false);
            }
            FIELD_SEP => {
                if let Some(last) = seen_separator.last_mut() {
                    *last = true;
                }
            }
            FIELD_END => {
                seen_separator.pop();
            }
            _ => {
                if seen_separator.is_empty() {
                    // Outside any field
                    result.push(c);
                } else if seen_separator.iter().all(|&s| s) {
                    // All nesting levels have seen their separator — output
                    result.push(c);
                }
                // else: inside a field code region — skip
            }
        }
    }

    result
}

/// Parse a field code string into (type, name).
/// Field code looks like: " DOCVARIABLE SDRDV_DTRDVVALEUR \* MERGEFORMAT"
fn parse_field_code(code: &str) -> (String, String) {
    let tokens: Vec<&str> = code
        .split_whitespace()
        .filter(|t| !t.starts_with('\\') && !t.starts_with('{'))
        .collect();

    let field_type = tokens.first().unwrap_or(&"").to_uppercase();
    let name = tokens.get(1).unwrap_or(&"").to_string();

    (field_type, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_field() {
        // \x13 DOCVARIABLE Foo \x14 bar \x15
        let chars: Vec<char> = "\x13 DOCVARIABLE Foo \x14bar\x15".chars().collect();
        let fields = extract_fields(&chars);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_type, "DOCVARIABLE");
        assert_eq!(fields[0].name, "Foo");
        assert_eq!(fields[0].value, "bar");
    }

    #[test]
    fn test_field_without_separator() {
        let chars: Vec<char> = "\x13 PAGE \x15".chars().collect();
        let fields = extract_fields(&chars);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field_type, "PAGE");
        assert_eq!(fields[0].name, "");
        assert_eq!(fields[0].value, "");
    }

    #[test]
    fn test_nested_fields() {
        // outer contains inner
        let chars: Vec<char> = "\x13 IF \x13 MERGEFIELD X \x14val\x15 \x14result\x15"
            .chars()
            .collect();
        let fields = extract_fields(&chars);
        assert_eq!(fields.len(), 2);
        // Inner field extracted first
        assert_eq!(fields[0].field_type, "MERGEFIELD");
        assert_eq!(fields[0].name, "X");
        assert_eq!(fields[0].value, "val");
        // Outer field
        assert_eq!(fields[1].field_type, "IF");
    }

    #[test]
    fn test_adjacent_fields() {
        let chars: Vec<char> = "\x13 PAGE \x14 1 \x15\x13 DATE \x14 2025 \x15"
            .chars()
            .collect();
        let fields = extract_fields(&chars);
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field_type, "PAGE");
        assert_eq!(fields[0].value, "1");
        assert_eq!(fields[1].field_type, "DATE");
        assert_eq!(fields[1].value, "2025");
    }

    #[test]
    fn test_strip_simple() {
        let chars: Vec<char> = "Hello \x13 PAGE \x14 5 \x15 world".chars().collect();
        let result = strip_field_codes(&chars);
        assert_eq!(result, "Hello  5  world");
    }

    #[test]
    fn test_strip_no_separator() {
        let chars: Vec<char> = "Hello \x13 PAGE \x15 world".chars().collect();
        let result = strip_field_codes(&chars);
        assert_eq!(result, "Hello  world");
    }

    #[test]
    fn test_strip_nested() {
        let chars: Vec<char> = "A\x13 IF \x13 X \x14v\x15 \x14result\x15B"
            .chars()
            .collect();
        let result = strip_field_codes(&chars);
        assert_eq!(result, "AresultB");
    }

    #[test]
    fn test_strip_no_fields() {
        let chars: Vec<char> = "plain text".chars().collect();
        let result = strip_field_codes(&chars);
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_parse_field_code_with_switches() {
        let (ft, name) = parse_field_code(" DOCVARIABLE MYVAR \\* MERGEFORMAT");
        assert_eq!(ft, "DOCVARIABLE");
        assert_eq!(name, "MYVAR");
    }
}
