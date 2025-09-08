use core::cmp;

use alloc::{format, string::String, string::ToString, vec::Vec};

pub mod nervape_constants;
pub mod types;
use serde_json::Value;
use types::{Error, Parameters, ParsedDNA, ParsedTrait, Pattern};

use crate::decoder::nervape_constants::{
    NERVAPE_COLOR_NAMES, NERVAPE_NOTES, NERVAPE_STRING_CONSTANTS,
};

use self::types::decode_trait_schema;

// example:
// argv[0] = efc2866a311da5b6dfcdfc4e3c22d00d024a53217ebc33855eeab1068990ed9d (hexed DNA string in Spore)
// argv[1] = d48869363ff41a103b131a29f43...d7be6eeaf513c2c3ae056b9b8c2e1 (hexed pattern string in Cluster)
pub fn dobs_parse_parameters(args: Vec<&[u8]>) -> Result<Parameters, Error> {
    if args.len() != 2 {
        return Err(Error::ParseInvalidArgCount);
    }

    let spore_dna = {
        let value = args[0];
        if value.is_empty() || value.len() % 2 != 0 {
            return Err(Error::ParseInvalidSporeDNA);
        }
        hex::decode(value).map_err(|_| Error::ParseInvalidSporeDNA)?
    };
    let traits_base = {
        let value = args[1];
        let traits_pool: Value =
            serde_json::from_slice(value).map_err(|_| Error::ParseInvalidTraitsBase)?;
        decode_trait_schema(traits_pool)?
    };
    Ok(Parameters {
        spore_dna,
        traits_base,
    })
}

pub fn dobs_decode(parameters: Parameters) -> Result<Vec<u8>, Error> {
    let Parameters {
        spore_dna,
        traits_base,
    } = parameters;

    let mut result = Vec::new();
    for schema_base in traits_base.into_iter() {
        let mut parsed_dna = ParsedDNA {
            name: schema_base.name,
            ..Default::default()
        };
        let byte_offset = cmp::min(schema_base.offset as usize, spore_dna.len());
        let byte_end = cmp::min(byte_offset + schema_base.len as usize, spore_dna.len());
        let mut dna_segment = spore_dna[byte_offset..byte_end].to_vec();
        let value: Value = match schema_base.pattern {
            Pattern::RawNumber => Value::Number(parse_u64(dna_segment)?.into()),
            Pattern::RawString => Value::String(hex::encode(&dna_segment)),
            Pattern::Utf8 => {
                while dna_segment.last() == Some(&0) {
                    dna_segment.pop();
                }
                Value::String(
                    String::from_utf8(dna_segment).map_err(|_| Error::DecodeBadUTF8Format)?,
                )
            }
            Pattern::Range => {
                let args = schema_base.args.ok_or(Error::DecodeMissingRangeArgs)?;
                if args.len() != 2 {
                    return Err(Error::DecodeInvalidRangeArgs);
                }
                let lower = args[0].as_u64().ok_or(Error::DecodeInvalidRangeArgs)?;
                let upper = args[1].as_u64().ok_or(Error::DecodeInvalidRangeArgs)?;
                if upper <= lower {
                    return Err(Error::DecodeInvalidRangeArgs);
                }
                let offset = parse_u64(dna_segment)?;
                let offset = offset % (upper - lower);
                Value::Number((lower + offset).into())
            }
            Pattern::Options => {
                let args = schema_base.args.ok_or(Error::DecodeMissingOptionArgs)?;
                if args.is_empty() {
                    return Err(Error::DecodeInvalidOptionArgs);
                }
                let offset = parse_u64(dna_segment)?;
                let offset = offset as usize % args.len();
                args[offset].clone()
            }
            Pattern::BtcFs => Value::String(format!("btcfs://{}i0", hex::encode(&dna_segment))),
            Pattern::BtcFs2 => Value::String(format!("btcfs://{}i1", hex::encode(&dna_segment))),
            Pattern::CkbFs => Value::String(format!("ckbfs://{}", hex::encode(&dna_segment))),
            Pattern::NervapeColor => {
                let color_index = parse_u8(dna_segment)?;
                if (color_index as usize) < NERVAPE_COLOR_NAMES.len() {
                    Value::String(NERVAPE_COLOR_NAMES[color_index as usize].to_string())
                } else {
                    Value::String("Other".to_string())
                }
            }
            Pattern::NervapeNote => {
                let index = parse_u16(dna_segment)?;
                if index as usize >= NERVAPE_NOTES.len() {
                    Value::String(String::default())
                } else {
                    Value::String(NERVAPE_NOTES[index as usize].to_string())
                }
            }
            Pattern::NervapeString => {
                let index = parse_u16(dna_segment)?;
                if index as usize >= NERVAPE_STRING_CONSTANTS.len() {
                    Value::String(String::default())
                } else {
                    Value::String(NERVAPE_STRING_CONSTANTS[index as usize].to_string())
                }
            }
            Pattern::NervapeInvolved => {
                let index = parse_u16(dna_segment)?;
                Value::String(String::default())
            }
            Pattern::NervapeSerialNumber => {
                todo!()
            }
        };
        parsed_dna.traits.push(ParsedTrait {
            type_: schema_base.type_,
            value,
        });
        result.push(parsed_dna);
    }

    Ok(serde_json::to_string(&result).unwrap().into_bytes())
}

fn parse_u64(dna_segment: Vec<u8>) -> Result<u64, Error> {
    let offset = match dna_segment.len() {
        1 => dna_segment[0] as u64,
        2 => u16::from_le_bytes(dna_segment.clone().try_into().unwrap()) as u64,
        3 | 4 => {
            let mut buf = [0u8; 4];
            buf[..dna_segment.len()].copy_from_slice(&dna_segment);
            u32::from_le_bytes(buf) as u64
        }
        5..=8 => {
            let mut buf = [0u8; 8];
            buf[..dna_segment.len()].copy_from_slice(&dna_segment);
            u64::from_le_bytes(buf)
        }
        _ => return Err(Error::DecodeUnexpectedDNASegment),
    };
    Ok(offset)
}
fn parse_u8(dna_segment: Vec<u8>) -> Result<u8, Error> {
    let offset = match dna_segment.len() {
        1 => dna_segment[0] as u8,
        2 => u16::from_le_bytes(dna_segment.clone().try_into().unwrap()) as u8,
        3 | 4 => {
            let mut buf = [0u8; 4];
            buf[..dna_segment.len()].copy_from_slice(&dna_segment);
            u32::from_le_bytes(buf) as u8
        }
        5..=8 => {
            let mut buf = [0u8; 8];
            buf[..dna_segment.len()].copy_from_slice(&dna_segment);
            u64::from_le_bytes(buf) as u8
        }
        _ => return Err(Error::DecodeUnexpectedDNASegment),
    };
    Ok(offset)
}

// Add parse_u16 function
fn parse_u16(dna_segment: Vec<u8>) -> Result<u16, Error> {
    let offset = match dna_segment.len() {
        1 => dna_segment[0] as u16,
        2 => u16::from_le_bytes(dna_segment.clone().try_into().unwrap()),
        3 | 4 => {
            let mut buf = [0u8; 4];
            buf[..dna_segment.len()].copy_from_slice(&dna_segment);
            u32::from_le_bytes(buf) as u16
        }
        5..=8 => {
            let mut buf = [0u8; 8];
            buf[..dna_segment.len()].copy_from_slice(&dna_segment);
            u64::from_le_bytes(buf) as u16
        }
        _ => return Err(Error::DecodeUnexpectedDNASegment),
    };
    Ok(offset)
}
