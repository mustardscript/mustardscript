use anyhow::{Result, anyhow, bail};
use indexmap::IndexMap;
use mustard::{HostError, StructuredValue, structured::StructuredNumber};
use mustard_bridge::{ResumeDto, RuntimeLimitsDto, StartOptionsDto};
use std::collections::BTreeMap;

const BOUNDARY_BINARY_MAGIC: [u8; 4] = [0x4d, 0x53, 0x42, 0x01];
const BOUNDARY_BINARY_KIND_START_OPTIONS: u8 = 1;
const BOUNDARY_BINARY_KIND_STRUCTURED_INPUTS: u8 = 2;
const BOUNDARY_BINARY_KIND_RESUME_PAYLOAD: u8 = 3;

const STRUCTURED_BINARY_TAG_UNDEFINED: u8 = 0;
const STRUCTURED_BINARY_TAG_NULL: u8 = 1;
const STRUCTURED_BINARY_TAG_HOLE: u8 = 2;
const STRUCTURED_BINARY_TAG_BOOL_FALSE: u8 = 3;
const STRUCTURED_BINARY_TAG_BOOL_TRUE: u8 = 4;
const STRUCTURED_BINARY_TAG_STRING: u8 = 5;
const STRUCTURED_BINARY_TAG_NUMBER_FINITE: u8 = 6;
const STRUCTURED_BINARY_TAG_NUMBER_NAN: u8 = 7;
const STRUCTURED_BINARY_TAG_NUMBER_INFINITY: u8 = 8;
const STRUCTURED_BINARY_TAG_NUMBER_NEG_INFINITY: u8 = 9;
const STRUCTURED_BINARY_TAG_NUMBER_NEG_ZERO: u8 = 10;
const STRUCTURED_BINARY_TAG_ARRAY: u8 = 11;
const STRUCTURED_BINARY_TAG_OBJECT: u8 = 12;

const RESUME_BINARY_TAG_VALUE: u8 = 0;
const RESUME_BINARY_TAG_ERROR: u8 = 1;
const RESUME_BINARY_TAG_CANCELLED: u8 = 2;

const HOST_BOUNDARY_MAX_DEPTH: usize = 128;
const HOST_BOUNDARY_MAX_ARRAY_LENGTH: usize = 1_000_000;

pub(crate) fn decode_start_options_bytes(bytes: &[u8]) -> Result<StartOptionsDto> {
    let mut decoder = BoundaryBinaryDecoder::new(bytes);
    decoder.expect_header(BOUNDARY_BINARY_KIND_START_OPTIONS)?;
    let inputs = decoder.read_inputs()?;
    let capabilities = decoder.read_string_vec()?;
    let limits = decoder.read_runtime_limits()?;
    decoder.finish()?;
    Ok(StartOptionsDto {
        inputs: inputs.into_iter().collect::<BTreeMap<_, _>>(),
        capabilities,
        limits,
    })
}

pub(crate) fn decode_structured_inputs_bytes(
    bytes: &[u8],
) -> Result<IndexMap<String, StructuredValue>> {
    let mut decoder = BoundaryBinaryDecoder::new(bytes);
    decoder.expect_header(BOUNDARY_BINARY_KIND_STRUCTURED_INPUTS)?;
    let inputs = decoder.read_inputs()?;
    decoder.finish()?;
    Ok(inputs)
}

pub(crate) fn decode_resume_payload_bytes(bytes: &[u8]) -> Result<ResumeDto> {
    let mut decoder = BoundaryBinaryDecoder::new(bytes);
    decoder.expect_header(BOUNDARY_BINARY_KIND_RESUME_PAYLOAD)?;
    let payload = decoder.read_resume_payload()?;
    decoder.finish()?;
    Ok(payload)
}

struct BoundaryBinaryDecoder<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> BoundaryBinaryDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    fn expect_header(&mut self, expected_kind: u8) -> Result<()> {
        let magic = self.read_exact(BOUNDARY_BINARY_MAGIC.len())?;
        if magic != BOUNDARY_BINARY_MAGIC {
            bail!("unsupported addon boundary binary header");
        }
        let actual_kind = self.read_u8()?;
        if actual_kind != expected_kind {
            bail!("unexpected addon boundary binary payload kind");
        }
        Ok(())
    }

    fn finish(&self) -> Result<()> {
        if self.cursor != self.bytes.len() {
            bail!("unexpected trailing bytes in addon boundary binary payload");
        }
        Ok(())
    }

    fn read_exact(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self
            .cursor
            .checked_add(len)
            .ok_or_else(|| anyhow!("addon boundary binary payload overflowed"))?;
        if end > self.bytes.len() {
            bail!("truncated addon boundary binary payload");
        }
        let slice = &self.bytes[self.cursor..end];
        self.cursor = end;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32> {
        let bytes = self.read_exact(4)?;
        Ok(u32::from_le_bytes(
            bytes.try_into().map_err(|_| anyhow!("invalid u32 field"))?,
        ))
    }

    fn read_f64(&mut self) -> Result<f64> {
        let bytes = self.read_exact(8)?;
        Ok(f64::from_le_bytes(
            bytes.try_into().map_err(|_| anyhow!("invalid f64 field"))?,
        ))
    }

    fn read_string(&mut self) -> Result<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_exact(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| anyhow!("addon boundary binary payload contained invalid utf-8"))
    }

    fn read_inputs(&mut self) -> Result<IndexMap<String, StructuredValue>> {
        let count = self.read_u32()? as usize;
        let mut inputs = IndexMap::with_capacity(count);
        for _ in 0..count {
            let key = self.read_string()?;
            let value = self.read_structured_value(1)?;
            inputs.insert(key, value);
        }
        Ok(inputs)
    }

    fn read_string_vec(&mut self) -> Result<Vec<String>> {
        let count = self.read_u32()? as usize;
        let mut values = Vec::with_capacity(count);
        for _ in 0..count {
            values.push(self.read_string()?);
        }
        Ok(values)
    }

    fn read_runtime_limits(&mut self) -> Result<RuntimeLimitsDto> {
        let mask = self.read_u8()?;
        let mut limits = RuntimeLimitsDto::default();
        if (mask & (1 << 0)) != 0 {
            limits.instruction_budget = Some(self.read_limit_value("instruction_budget")?);
        }
        if (mask & (1 << 1)) != 0 {
            limits.heap_limit_bytes = Some(self.read_limit_value("heap_limit_bytes")?);
        }
        if (mask & (1 << 2)) != 0 {
            limits.allocation_budget = Some(self.read_limit_value("allocation_budget")?);
        }
        if (mask & (1 << 3)) != 0 {
            limits.call_depth_limit = Some(self.read_limit_value("call_depth_limit")?);
        }
        if (mask & (1 << 4)) != 0 {
            limits.max_outstanding_host_calls =
                Some(self.read_limit_value("max_outstanding_host_calls")?);
        }
        Ok(limits)
    }

    fn read_limit_value(&mut self, field_name: &str) -> Result<usize> {
        let value = self.read_f64()?;
        if !value.is_finite() || value < 0.0 || value.fract() != 0.0 {
            bail!("{field_name} must be a non-negative integer");
        }
        if value > (usize::MAX as f64) {
            bail!("{field_name} exceeds platform limits");
        }
        Ok(value as usize)
    }

    fn read_resume_payload(&mut self) -> Result<ResumeDto> {
        match self.read_u8()? {
            RESUME_BINARY_TAG_VALUE => Ok(ResumeDto::Value {
                value: self.read_structured_value(1)?,
            }),
            RESUME_BINARY_TAG_ERROR => {
                let name = self.read_string()?;
                let message = self.read_string()?;
                let code = if self.read_u8()? == 0 {
                    None
                } else {
                    Some(self.read_string()?)
                };
                let details = if self.read_u8()? == 0 {
                    None
                } else {
                    Some(self.read_structured_value(1)?)
                };
                Ok(ResumeDto::Error {
                    error: HostError {
                        name,
                        message,
                        code,
                        details,
                    },
                })
            }
            RESUME_BINARY_TAG_CANCELLED => Ok(ResumeDto::Cancelled),
            _ => bail!("unsupported addon boundary resume payload variant"),
        }
    }

    fn read_structured_value(&mut self, depth: usize) -> Result<StructuredValue> {
        if depth > HOST_BOUNDARY_MAX_DEPTH {
            bail!("host boundary nesting limit exceeded");
        }
        match self.read_u8()? {
            STRUCTURED_BINARY_TAG_UNDEFINED => Ok(StructuredValue::Undefined),
            STRUCTURED_BINARY_TAG_NULL => Ok(StructuredValue::Null),
            STRUCTURED_BINARY_TAG_HOLE => Ok(StructuredValue::Hole),
            STRUCTURED_BINARY_TAG_BOOL_FALSE => Ok(StructuredValue::Bool(false)),
            STRUCTURED_BINARY_TAG_BOOL_TRUE => Ok(StructuredValue::Bool(true)),
            STRUCTURED_BINARY_TAG_STRING => Ok(StructuredValue::String(self.read_string()?)),
            STRUCTURED_BINARY_TAG_NUMBER_FINITE => Ok(StructuredValue::Number(
                StructuredNumber::Finite(self.read_f64()?),
            )),
            STRUCTURED_BINARY_TAG_NUMBER_NAN => Ok(StructuredValue::Number(StructuredNumber::NaN)),
            STRUCTURED_BINARY_TAG_NUMBER_INFINITY => {
                Ok(StructuredValue::Number(StructuredNumber::Infinity))
            }
            STRUCTURED_BINARY_TAG_NUMBER_NEG_INFINITY => {
                Ok(StructuredValue::Number(StructuredNumber::NegInfinity))
            }
            STRUCTURED_BINARY_TAG_NUMBER_NEG_ZERO => {
                Ok(StructuredValue::Number(StructuredNumber::NegZero))
            }
            STRUCTURED_BINARY_TAG_ARRAY => {
                let len = self.read_u32()? as usize;
                if len > HOST_BOUNDARY_MAX_ARRAY_LENGTH {
                    bail!(
                        "host boundary arrays longer than {HOST_BOUNDARY_MAX_ARRAY_LENGTH} elements cannot cross the host boundary"
                    );
                }
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push(self.read_structured_value(depth + 1)?);
                }
                Ok(StructuredValue::Array(values))
            }
            STRUCTURED_BINARY_TAG_OBJECT => {
                let len = self.read_u32()? as usize;
                let mut values = IndexMap::with_capacity(len);
                for _ in 0..len {
                    let key = self.read_string()?;
                    let value = self.read_structured_value(depth + 1)?;
                    values.insert(key, value);
                }
                Ok(StructuredValue::Object(values))
            }
            _ => bail!("unsupported addon boundary structured value tag"),
        }
    }
}
