//! Reading and writing CSV (RFC 4180), and guessing column types for an import.

/// Parse CSV text into rows of fields. Handles quoted fields with commas,
/// doubled quotes and line breaks, CRLF endings and a UTF-8 byte order mark.
pub fn parse(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = vec![];
    let mut row = vec![];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            ',' => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                any = false;
            }
            _ => field.push(c),
        }
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    rows
}

/// One CSV line, quoting fields that need it, ending in CRLF.
pub fn line(fields: &[String]) -> String {
    let mut out = fields
        .iter()
        .map(|f| {
            if f.contains([',', '"', '\n', '\r']) || f.starts_with(' ') || f.ends_with(' ') {
                format!("\"{}\"", f.replace('"', "\"\""))
            } else {
                f.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    out.push_str("\r\n");
    out
}

/// A guessed column type for imported values.
#[derive(Debug, PartialEq)]
pub enum Guess {
    Text,
    Number,
    Checkbox,
    Date,
    Url,
    Select,
    MultiSelect,
}

/// A date as `YYYY-MM-DD` from the formats exports use: ISO, Notion's
/// `November 6, 2026` (with an optional time after it), and `06/11/2026`
/// (day first, as in the UK).
pub fn date(s: &str) -> Option<String> {
    let s = s.trim();
    let head = s.split(" → ").next().unwrap_or(s);
    for fmt in ["%Y-%m-%d", "%B %d, %Y", "%b %d, %Y", "%d/%m/%Y"] {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(head, fmt) {
            return Some(d.format("%Y-%m-%d").to_string());
        }
    }
    // "November 6, 2026 3:30 PM": keep the date.
    let mut words = head.split(' ');
    let first3 = [words.next(), words.next(), words.next()];
    if let [Some(m), Some(d), Some(y)] = first3 {
        let joined = format!("{m} {d} {y}");
        if let Ok(d) = chrono::NaiveDate::parse_from_str(&joined, "%B %d, %Y") {
            return Some(d.format("%Y-%m-%d").to_string());
        }
    }
    None
}

pub fn number(s: &str) -> Option<f64> {
    let t = s.trim().trim_start_matches(['£', '$', '€']).replace(',', "");
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

pub fn checkbox(s: &str) -> Option<bool> {
    match s.trim().to_lowercase().as_str() {
        "yes" | "true" | "checked" | "x" => Some(true),
        "no" | "false" | "unchecked" | "" => Some(false),
        _ => None,
    }
}

/// Guess a type from a column's values. Empty cells say nothing.
pub fn guess(values: &[&str]) -> Guess {
    let filled: Vec<&str> = values.iter().map(|v| v.trim()).filter(|v| !v.is_empty()).collect();
    if filled.is_empty() {
        return Guess::Text;
    }
    let lower: Vec<String> = filled.iter().map(|v| v.to_lowercase()).collect();
    if lower.iter().all(|v| matches!(v.as_str(), "yes" | "no" | "true" | "false" | "checked" | "unchecked")) {
        return Guess::Checkbox;
    }
    if filled.iter().all(|v| number(v).is_some()) {
        return Guess::Number;
    }
    if filled.iter().all(|v| date(v).is_some()) {
        return Guess::Date;
    }
    if lower.iter().all(|v| v.starts_with("http://") || v.starts_with("https://")) {
        return Guess::Url;
    }
    let short = filled.iter().all(|v| v.chars().count() <= 40 && !v.contains('\n'));
    if short && filled.len() >= 3 {
        let multi = filled.iter().any(|v| v.contains(", "));
        let mut distinct: Vec<&str> = filled.iter().flat_map(|v| v.split(", ")).map(str::trim).collect();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() <= 12 && (multi || distinct.len() * 2 <= filled.len().max(4)) {
            return if multi { Guess::MultiSelect } else { Guess::Select };
        }
    }
    Guess::Text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quotes_and_breaks() {
        let rows = parse("\u{feff}Name,Notes\r\n\"Kit, copper\",\"Say \"\"hi\"\"\nthen\"\r\nFluid,\r\n");
        assert_eq!(
            rows,
            vec![
                vec!["Name".to_string(), "Notes".into()],
                vec!["Kit, copper".into(), "Say \"hi\"\nthen".into()],
                vec!["Fluid".into(), "".into()],
            ]
        );
        assert_eq!(line(&["a,b".into(), "c".into()]), "\"a,b\",c\r\n");
    }

    #[test]
    fn guesses_types() {
        assert_eq!(guess(&["42", "£9.99", ""]), Guess::Number);
        assert_eq!(guess(&["Yes", "No"]), Guess::Checkbox);
        assert_eq!(guess(&["November 6, 2026", "2026-11-07"]), Guess::Date);
        assert_eq!(guess(&["To order", "Ordered", "To order", "Ordered"]), Guess::Select);
        assert_eq!(guess(&["brakes, engine", "engine", "paint", "brakes"]), Guess::MultiSelect);
        assert_eq!(guess(&["a long free text", "another", "third one"]), Guess::Text);
        assert_eq!(date("November 6, 2026 3:30 PM").as_deref(), Some("2026-11-06"));
    }
}
