//! Task and group ids: ULIDs, 48 bits of millisecond time then 80 random bits, as 26 Crockford base32 characters.

use anyhow::Result;
use rand::RngCore;

const ULID_ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A new id that sorts by creation time.
pub(crate) fn new_task_id() -> Result<String> {
    let mut raw = [0u8; 16];
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    raw[0] = (ms >> 40) as u8;
    raw[1] = (ms >> 32) as u8;
    raw[2] = (ms >> 24) as u8;
    raw[3] = (ms >> 16) as u8;
    raw[4] = (ms >> 8) as u8;
    raw[5] = ms as u8;
    rand::rngs::OsRng.try_fill_bytes(&mut raw[6..])?;
    Ok(encode_ulid(&raw))
}

pub(crate) fn encode_ulid(id: &[u8; 16]) -> String {
    let mut dst = [0u8; 26];
    let mut acc: usize = 0;
    let mut bits: usize = 0;
    let mut pos: usize = 25;
    for i in (0..16).rev() {
        acc |= (id[i] as usize) << bits;
        bits += 8;
        while bits >= 5 {
            dst[pos] = ULID_ALPHABET[acc & 31];
            pos = pos.wrapping_sub(1);
            acc >>= 5;
            bits -= 5;
        }
    }
    dst[pos] = ULID_ALPHABET[acc & 31];
    String::from_utf8_lossy(&dst).into_owned()
}
