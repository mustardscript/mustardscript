use super::*;

const URI_RESERVED: &[u8] = b";/?:@&=+$,#";
const URI_UNESCAPED: &[u8] = b"-_.!~*'()";

fn malformed_uri() -> MustardError {
    MustardError::runtime("URIError: malformed URI sequence")
}

fn percent_byte(bytes: &[u8], index: usize) -> MustardResult<u8> {
    if bytes.get(index) != Some(&b'%') {
        return Err(malformed_uri());
    }
    let digit = |offset| {
        bytes
            .get(index + offset)
            .and_then(|byte| (*byte as char).to_digit(16))
            .ok_or_else(malformed_uri)
    };
    Ok((digit(1)? * 16 + digit(2)?) as u8)
}

impl Runtime {
    pub(crate) fn call_uri_codec(
        &mut self,
        args: &[Value],
        encode: bool,
        component: bool,
    ) -> MustardResult<Value> {
        self.with_temporary_roots(args, |runtime| {
            let input = runtime.to_string(args.first().cloned().unwrap_or(Value::Undefined))?;
            runtime.charge_native_helper_work(input.len())?;
            let bytes = input.as_bytes();
            let result = if encode {
                let unescaped = |byte: u8| {
                    byte.is_ascii_alphanumeric()
                        || URI_UNESCAPED.contains(&byte)
                        || (!component && URI_RESERVED.contains(&byte))
                };
                let length = bytes.iter().try_fold(0usize, |total, byte| {
                    total
                        .checked_add(if unescaped(*byte) { 1 } else { 3 })
                        .ok_or_else(|| limit_error("heap limit exceeded"))
                })?;
                runtime.ensure_heap_capacity(length)?;
                let mut output = Vec::with_capacity(length);
                const HEX: &[u8] = b"0123456789ABCDEF";
                for &byte in bytes {
                    if unescaped(byte) {
                        output.push(byte);
                    } else {
                        output.extend_from_slice(&[
                            b'%',
                            HEX[(byte >> 4) as usize],
                            HEX[(byte & 15) as usize],
                        ]);
                    }
                }
                String::from_utf8(output).expect("URI encoding produces ASCII")
            } else {
                runtime.ensure_heap_capacity(input.len())?;
                let mut output = Vec::with_capacity(input.len());
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != b'%' {
                        output.push(bytes[index]);
                        index += 1;
                        continue;
                    }
                    let first = percent_byte(bytes, index)?;
                    if first.is_ascii() {
                        if !component && URI_RESERVED.contains(&first) {
                            output.extend_from_slice(&bytes[index..index + 3]);
                        } else {
                            output.push(first);
                        }
                        index += 3;
                        continue;
                    }
                    let width = match first {
                        0xC2..=0xDF => 2,
                        0xE0..=0xEF => 3,
                        0xF0..=0xF4 => 4,
                        _ => return Err(malformed_uri()),
                    };
                    let mut sequence = [0u8; 4];
                    sequence[0] = first;
                    for (offset, byte) in sequence.iter_mut().enumerate().take(width).skip(1) {
                        *byte = percent_byte(bytes, index + offset * 3)?;
                    }
                    let sequence = &sequence[..width];
                    std::str::from_utf8(sequence).map_err(|_| malformed_uri())?;
                    output.extend_from_slice(sequence);
                    index += width * 3;
                }
                String::from_utf8(output).map_err(|_| malformed_uri())?
            };
            Ok(Value::String(result))
        })
    }
}
