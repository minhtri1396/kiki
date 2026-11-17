//! Scalar typing: what an unquoted YAML scalar resolves to (null, bool, number, timestamp or string).
//!
//! These are the resolution rules of yaml.v3, shared by the reader (to decode a plain scalar
//! into a typed field) and by the emitter (to quote a string that would read back as another type).

use std::sync::LazyLock;

use regex::Regex;

/// What a plain scalar resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Timestamp,
    Str,
}

static YAML_FLOAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?$").unwrap());
static BASE60_FLOAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+(?:\.[0-9_]*)?$").unwrap());
static DOT_FLOAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\.[0-9]+([eE][-+]?[0-9]+)?$").unwrap());
static TS_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})$").unwrap());
static TS_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^([0-9]{4})-([0-9]{1,2})-([0-9]{1,2})([Tt ])([0-9]{1,2}):([0-9]{1,2}):([0-9]{1,2})(?:[.,][0-9]+)?(Z|[-+][0-9]{2}:[0-9]{2})?$",
    )
    .unwrap()
});

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
    }
}

/// The timestamp formats yaml.v3 recognises: `2024-1-2T3:4:5.999999999Z`, the same with a
/// lower case `t`, `2006-1-2 15:4:5.999999999` (no zone) and `2006-1-2`.
fn parse_timestamp(s: &str) -> bool {
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits != 4 || digits == s.len() || s.as_bytes()[digits] != b'-' {
        return false;
    }
    let num = |m: &regex::Captures, i: usize| m[i].parse::<u32>().unwrap_or(u32::MAX);
    if let Some(m) = TS_DATE.captures(s) {
        let (y, mo, d) = (num(&m, 1), num(&m, 2), num(&m, 3));
        return (1..=12).contains(&mo) && d >= 1 && d <= days_in_month(y, mo);
    }
    if let Some(m) = TS_TIME.captures(s) {
        let (y, mo, d) = (num(&m, 1), num(&m, 2), num(&m, 3));
        let sep = &m[4];
        let zone = m.get(8).map(|z| z.as_str());
        // A space separated value has no zone; `T`/`t` values need one.
        if (sep == " " && zone.is_some()) || (sep != " " && zone.is_none()) {
            return false;
        }
        let (h, mi, sec) = (num(&m, 5), num(&m, 6), num(&m, 7));
        if !(1..=12).contains(&mo) || d < 1 || d > days_in_month(y, mo) || h > 23 || mi > 59 || sec > 59 {
            return false;
        }
        if let Some(z) = zone {
            if z != "Z" {
                let zh: u32 = z[1..3].parse().unwrap_or(99);
                let zm: u32 = z[4..6].parse().unwrap_or(99);
                if zh > 24 || zm > 60 {
                    return false;
                }
            }
        }
        return true;
    }
    false
}

/// Parses an integer with base prefixes (`0x`, `0o`, `0b`, leading `0` is octal) that fits `i64` or `u64`.
fn parse_int_base0(s: &str) -> Option<i128> {
    let (neg, rest) = match s.as_bytes().first()? {
        b'-' => (true, &s[1..]),
        b'+' => (false, &s[1..]),
        _ => (false, s),
    };
    if rest.is_empty() {
        return None;
    }
    let (base, digits) = if rest.as_bytes()[0] == b'0' {
        let lower = |i: usize| rest.as_bytes()[i].to_ascii_lowercase();
        if rest.len() >= 3 && lower(1) == b'b' {
            (2u32, &rest[2..])
        } else if rest.len() >= 3 && lower(1) == b'o' {
            (8, &rest[2..])
        } else if rest.len() >= 3 && lower(1) == b'x' {
            (16, &rest[2..])
        } else {
            (8, &rest[1..])
        }
    } else {
        (10, rest)
    };
    let mut value: u128 = 0;
    if digits.is_empty() && !(base == 8 && rest.len() == 1) {
        return None;
    }
    for c in digits.chars() {
        let d = c.to_digit(base)?;
        value = value.checked_mul(base as u128)?.checked_add(d as u128)?;
        if value > u64::MAX as u128 {
            return None;
        }
    }
    if neg {
        if value > (i64::MAX as u128) + 1 {
            return None;
        }
        return Some(-(value as i128));
    }
    Some(value as i128)
}

fn parse_float_finite(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|f| f.is_finite())
}

/// yaml.v3's `resolve("", in)` for an untagged plain scalar.
pub fn resolve(input: &str) -> Resolved {
    let hint = match input.as_bytes().first() {
        None => b'N',
        Some(b) => match b {
            b'+' | b'-' => b'S',
            b'0'..=b'9' => b'D',
            b'y' | b'Y' | b'n' | b'N' | b't' | b'T' | b'f' | b'F' | b'o' | b'O' | b'~' => b'M',
            b'.' => b'.',
            _ => 0,
        },
    };
    if hint == 0 {
        return Resolved::Str;
    }
    match input {
        "true" | "True" | "TRUE" => return Resolved::Bool(true),
        "false" | "False" | "FALSE" => return Resolved::Bool(false),
        "" | "~" | "null" | "Null" | "NULL" => return Resolved::Null,
        ".nan" | ".NaN" | ".NAN" => return Resolved::Float(f64::NAN),
        ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF" => return Resolved::Float(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => return Resolved::Float(f64::NEG_INFINITY),
        _ => {}
    }
    match hint {
        b'.' if DOT_FLOAT.is_match(input) => {
            if let Some(f) = parse_float_finite(input) {
                return Resolved::Float(f);
            }
        }
        b'D' | b'S' => {
            if parse_timestamp(input) {
                return Resolved::Timestamp;
            }
            let plain = input.replace('_', "");
            if let Some(v) = parse_int_base0(&plain) {
                return Resolved::Int(v);
            }
            if YAML_FLOAT.is_match(&plain) {
                if let Some(f) = parse_float_finite(&plain) {
                    return Resolved::Float(f);
                }
            }
        }
        _ => {}
    }
    Resolved::Str
}

pub(crate) fn is_old_bool(s: &str) -> bool {
    matches!(
        s,
        "y" | "Y" | "yes" | "Yes" | "YES" | "on" | "On" | "ON" | "n" | "N" | "no" | "No" | "NO" | "off" | "Off" | "OFF"
    )
}

pub(crate) fn is_base60_float(s: &str) -> bool {
    let Some(c) = s.bytes().next() else { return false };
    if !(c == b'+' || c == b'-' || c.is_ascii_digit()) || !s.contains(':') {
        return false;
    }
    BASE60_FLOAT.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_scalars_resolve_like_yaml_v3() {
        assert_eq!(resolve(""), Resolved::Null);
        assert_eq!(resolve("~"), Resolved::Null);
        assert_eq!(resolve("True"), Resolved::Bool(true));
        assert_eq!(resolve("12"), Resolved::Int(12));
        assert_eq!(resolve("0o17"), Resolved::Int(15));
        assert_eq!(resolve("1_000"), Resolved::Int(1000));
        assert_eq!(resolve("1.5"), Resolved::Float(1.5));
        assert_eq!(resolve("2020-01-02"), Resolved::Timestamp);
        assert_eq!(resolve("2020-02-30"), Resolved::Str);
        assert_eq!(resolve("hello"), Resolved::Str);
        assert_eq!(
            resolve("yes"),
            Resolved::Str,
            "yes is only a bool when the target type is bool"
        );
    }

    #[test]
    fn base60_floats_and_old_bools() {
        assert!(is_base60_float("1:20"));
        assert!(!is_base60_float("a:20"));
        assert!(is_old_bool("on"));
        assert!(!is_old_bool("true"));
    }
}
