//! Fractional indexing: sort keys that always leave room between two items,
//! so moving a card rewrites only that card's row.
//!
//! Keys are strings over `0-9A-Za-z`, which sort the same way as bytes, so
//! SQLite's default `ORDER BY position` keeps them in order.

const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn digit(c: u8) -> usize {
    DIGITS.iter().position(|&d| d == c).expect("position keys use 0-9A-Za-z")
}

/// A key strictly between `a` and `b`. `None` means the start or the end.
/// Panics if `a >= b`; callers pass neighbours read from the database.
pub fn between(a: Option<&str>, b: Option<&str>) -> String {
    let a = a.unwrap_or("");
    if let Some(b) = b {
        assert!(a < b, "position keys out of order: {a:?} >= {b:?}");
    }
    midpoint(a.as_bytes(), b.map(str::as_bytes))
}

fn midpoint(a: &[u8], b: Option<&[u8]>) -> String {
    if let Some(b) = b {
        // Shared prefix (a padded with zeros) stays as it is.
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(b'0') == b[n] {
            n += 1;
        }
        if n > 0 {
            let rest_a = if n < a.len() { &a[n..] } else { &[][..] };
            return format!("{}{}", String::from_utf8_lossy(&b[..n]), midpoint(rest_a, Some(&b[n..])));
        }
    }
    let da = a.first().map(|&c| digit(c)).unwrap_or(0);
    let db = b.and_then(|b| b.first()).map(|&c| digit(c)).unwrap_or(DIGITS.len());
    if db - da > 1 {
        (DIGITS[(da + db).div_ceil(2)] as char).to_string()
    } else if let Some(b) = b.filter(|b| b.len() > 1) {
        (b[0] as char).to_string()
    } else {
        let rest = if a.len() > 1 { &a[1..] } else { &[][..] };
        format!("{}{}", DIGITS[da] as char, midpoint(rest, None))
    }
}

/// `count` evenly spread keys, for seeding columns.
pub fn spread(count: usize) -> Vec<String> {
    let mut keys = Vec::with_capacity(count);
    let mut last: Option<String> = None;
    for _ in 0..count {
        let k = between(last.as_deref(), None);
        keys.push(k.clone());
        last = Some(k);
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_between_their_neighbours() {
        let pairs: &[(Option<&str>, Option<&str>)] = &[
            (None, None),
            (None, Some("a0")),
            (Some("a0"), Some("a1")),
            (Some("a0"), Some("a0V")),
            (Some("a2"), None),
            (Some("z"), None),
            (Some("zzzz"), None),
            (None, Some("01")),
            (Some("V"), Some("W")),
            (Some("V"), Some("V1")),
        ];
        for &(a, b) in pairs {
            let k = between(a, b);
            if let Some(a) = a {
                assert!(a < k.as_str(), "{a:?} < {k:?}");
            }
            if let Some(b) = b {
                assert!(k.as_str() < b, "{k:?} < {b:?}");
            }
            assert!(!k.ends_with('0'), "{k:?} has a trailing zero");
        }
    }

    #[test]
    fn repeated_inserts_stay_ordered() {
        // Always insert at the front, then always between the first two.
        let mut keys = vec![between(None, None)];
        for _ in 0..200 {
            let k = between(None, Some(&keys[0]));
            keys.insert(0, k);
        }
        for _ in 0..200 {
            let k = between(Some(&keys[0]), Some(&keys[1]));
            keys.insert(1, k);
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(deduped.len(), keys.len());
    }

    #[test]
    fn spread_is_ordered() {
        let keys = spread(5);
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
