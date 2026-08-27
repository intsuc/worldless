use serde::Deserialize;
use serde_json::{Map, Value};

use crate::nbt::JavaString;

pub(crate) fn parse(contents: &str) -> Result<Value, String> {
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let encoded = encode_strings_as_utf16(contents)?;
    validate_nesting(&encoded)?;

    let mut deserializer = serde_json::Deserializer::from_str(&encoded);
    deserializer.disable_recursion_limit();
    let value =
        Value::deserialize(&mut deserializer).map_err(|error| format!("invalid JSON: {error}"))?;
    deserializer
        .end()
        .map_err(|error| format!("invalid JSON: {error}"))?;
    Ok(value)
}

pub(crate) fn field<'a>(object: &'a Map<String, Value>, name: &str) -> Option<&'a Value> {
    object.get(&encode_utf16_units(
        &name.encode_utf16().collect::<Vec<_>>(),
    ))
}

pub(crate) fn decode_string(encoded: &str) -> JavaString {
    let bytes = encoded.as_bytes();
    assert_eq!(bytes.first(), Some(&b'u'));
    assert_eq!((bytes.len() - 1) % 4, 0);
    let (chunks, remainder) = bytes[1..].as_chunks::<4>();
    assert!(remainder.is_empty());
    let units = chunks
        .iter()
        .map(|digits| {
            digits.iter().fold(0_u16, |value, digit| {
                value * 16
                    + u16::from(match digit {
                        b'0'..=b'9' => *digit - b'0',
                        b'a'..=b'f' => *digit - b'a' + 10,
                        _ => unreachable!("encoded JSON strings contain lowercase hex digits"),
                    })
            })
        })
        .collect();
    JavaString::from_units(units)
}

fn validate_nesting(encoded: &str) -> Result<(), String> {
    const MAX_NESTING: usize = 255;

    let mut depth = 0;
    let mut in_string = false;
    for byte in encoded.bytes() {
        match byte {
            b'"' => in_string = !in_string,
            b'{' | b'[' if !in_string => {
                depth += 1;
                if depth > MAX_NESTING {
                    return Err(format!("invalid JSON: nesting limit {MAX_NESTING} reached"));
                }
            }
            b'}' | b']' if !in_string => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

fn encode_strings_as_utf16(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'"' {
            let character = input[cursor..]
                .chars()
                .next()
                .expect("the cursor is below the UTF-8 string length");
            output.push(character);
            cursor += character.len_utf8();
            continue;
        }

        cursor += 1;
        let mut units = Vec::new();
        loop {
            let Some(&byte) = bytes.get(cursor) else {
                return Err("invalid JSON: unterminated string".to_owned());
            };
            match byte {
                b'"' => {
                    cursor += 1;
                    break;
                }
                b'\\' => {
                    cursor += 1;
                    let Some(&escape) = bytes.get(cursor) else {
                        return Err("invalid JSON: unterminated escape".to_owned());
                    };
                    cursor += 1;
                    match escape {
                        b'"' | b'\\' | b'/' => units.push(u16::from(escape)),
                        b'b' => units.push(0x08),
                        b'f' => units.push(0x0c),
                        b'n' => units.push(0x0a),
                        b'r' => units.push(0x0d),
                        b't' => units.push(0x09),
                        b'u' => {
                            let digits = bytes.get(cursor..cursor + 4).ok_or_else(|| {
                                "invalid JSON: incomplete Unicode escape".to_owned()
                            })?;
                            let mut unit = 0_u16;
                            for digit in digits {
                                unit = unit
                                    .checked_mul(16)
                                    .expect("four hexadecimal digits fit in a UTF-16 code unit");
                                unit += match digit {
                                    b'0'..=b'9' => u16::from(*digit - b'0'),
                                    b'a'..=b'f' => u16::from(*digit - b'a' + 10),
                                    b'A'..=b'F' => u16::from(*digit - b'A' + 10),
                                    _ => {
                                        return Err(
                                            "invalid JSON: invalid Unicode escape".to_owned()
                                        );
                                    }
                                };
                            }
                            units.push(unit);
                            cursor += 4;
                        }
                        _ => return Err("invalid JSON: invalid escape".to_owned()),
                    }
                }
                0x00..=0x1f => {
                    return Err("invalid JSON: unescaped control character".to_owned());
                }
                _ => {
                    let character = input[cursor..]
                        .chars()
                        .next()
                        .expect("the cursor is on a UTF-8 character boundary");
                    units.extend(character.encode_utf16(&mut [0; 2]).iter().copied());
                    cursor += character.len_utf8();
                }
            }
        }
        output.push('"');
        output.push_str(&encode_utf16_units(&units));
        output.push('"');
    }
    Ok(output)
}

fn encode_utf16_units(units: &[u16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(1 + units.len() * 4);
    encoded.push('u');
    for unit in units {
        for shift in [12, 8, 4, 0] {
            encoded.push(char::from(HEX[usize::from((unit >> shift) & 0xf)]));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_java_utf16_strings_and_accepts_a_leading_bom() {
        let value = parse("\u{feff}{\"value\":\"\\uD800\"}").unwrap();
        let value = field(value.as_object().unwrap(), "value")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(decode_string(value).units(), [0xd800]);
    }

    #[test]
    fn enforces_gson_json_reader_nesting_limit() {
        fn nested_array(depth: usize) -> String {
            format!("{}0{}", "[".repeat(depth), "]".repeat(depth))
        }

        assert!(parse(&nested_array(255)).is_ok());
        assert!(
            parse(&nested_array(256))
                .unwrap_err()
                .contains("nesting limit 255")
        );
    }
}
