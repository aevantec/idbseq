//! Value encoding for redb tables: JSON (readable, and tolerant of added
//! fields) followed by a checksum, so a damaged record is refused instead of
//! read. A counter that silently went backwards would issue IDs again.
//!
//! Layout: `<json>\n#<16 hex digits: FNV-1a 64 of the json>`. Records written
//! before checksums existed are plain JSON; they are read as they are and get a
//! checksum the next time they are written.

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::store::StoreError;

/// Between the JSON and its checksum. Compact JSON never contains a raw newline.
const MARK: &[u8] = b"\n#";
const SUM_DIGITS: usize = 16;
const TRAILER: usize = MARK.len() + SUM_DIGITS;

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
    let json = serde_json::to_vec(value).map_err(backend)?;
    let sum = format!("{:016x}", fnv1a64(&json));
    Ok([json.as_slice(), MARK, sum.as_bytes()].concat())
}

pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, StoreError> {
    serde_json::from_slice(verified(bytes)?).map_err(|e| damaged(&e))
}

/// The JSON of a record, after checking its checksum (if it has one).
fn verified(bytes: &[u8]) -> Result<&[u8], StoreError> {
    let Some(split) = bytes.len().checked_sub(TRAILER) else {
        return Ok(bytes);
    };
    let (json, trailer) = bytes.split_at(split);
    let Some(digits) = trailer.strip_prefix(MARK) else {
        return Ok(bytes); // written before checksums
    };
    let expected = std::str::from_utf8(digits)
        .ok()
        .and_then(|digits| u64::from_str_radix(digits, 16).ok());
    if expected == Some(fnv1a64(json)) {
        Ok(json)
    } else {
        Err(damaged(&"checksum mismatch"))
    }
}

/// FNV-1a, 64-bit: tiny, stable across releases and platforms, and enough to
/// catch accidental damage (it is not meant to stop deliberate edits).
fn fnv1a64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(PRIME)
    })
}

fn damaged(detail: &dyn std::fmt::Display) -> StoreError {
    StoreError::Backend(format!(
        "a stored record is damaged ({detail}); restore the file from a backup"
    ))
}

/// Any backend failure, with its message.
#[allow(clippy::needless_pass_by_value)] // used as `map_err(backend)`
pub(crate) fn backend(error: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Record {
        last_id: i64,
    }

    #[test]
    fn a_record_round_trips_with_its_checksum() {
        let bytes = encode(&Record { last_id: 41 }).expect("encode");

        assert!(bytes.starts_with(br#"{"last_id":41}"#));
        assert_eq!(
            decode::<Record>(&bytes).expect("decode"),
            Record { last_id: 41 }
        );
    }

    #[test]
    fn one_changed_byte_is_refused_not_read() {
        let mut bytes = encode(&Record { last_id: 1 }).expect("encode");
        let digit = bytes.iter().position(|&b| b == b'1').expect("digit");
        bytes[digit] = b'0'; // a counter one lower would issue an ID again

        let read = decode::<Record>(&bytes);

        assert!(matches!(read, Err(StoreError::Backend(message)) if message.contains("damaged")));
    }

    #[test]
    fn a_damaged_checksum_or_mark_is_refused() {
        let good = encode(&Record { last_id: 7 }).expect("encode");
        for at in [good.len() - 1, good.len() - TRAILER] {
            let mut bad = good.clone();
            bad[at] ^= 0x01;
            assert!(decode::<Record>(&bad).is_err(), "byte {at}");
        }
    }

    #[test]
    fn records_written_before_checksums_are_still_read() {
        assert_eq!(
            decode::<Record>(br#"{"last_id":3}"#).expect("legacy"),
            Record { last_id: 3 }
        );
    }
}
