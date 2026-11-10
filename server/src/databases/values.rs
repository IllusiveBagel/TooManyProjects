//! Cell values: checking what goes in, and the filters and sorts a view applies.
//!
//! A row's values are a JSON object keyed by column id. Each kind keeps one
//! JSON shape: text, url, date (`YYYY-MM-DD`), select (a choice id) and person
//! (a user id) are strings; number and currency are numbers; checkbox is a
//! boolean; multi-select is an array of choice ids. Relations are not stored
//! here but in `db_relations`, and reach the filters as an array of ids.

use std::cmp::Ordering;

use serde_json::Value;

use super::{Column, Filter, Sort};
use crate::error::{AppError, AppResult};

pub const KINDS: &[&str] = &[
    "text",
    "number",
    "currency",
    "select",
    "multi_select",
    "date",
    "checkbox",
    "url",
    "person",
    "relation",
];
pub const VIEW_KINDS: &[&str] = &["table", "board", "calendar", "gallery"];
pub const RELATION_TARGETS: &[&str] = &["task", "note", "project", "row"];
pub const FILTER_OPS: &[&str] = &["is", "is_not", "contains", "empty", "not_empty", "gt", "lt"];

fn bad(column: &Column, why: &str) -> AppError {
    AppError::BadRequest("invalid_value", format!("{}: {why}", column.name))
}

/// The value to store for `column`, or `None` to clear the cell. People are
/// checked by the caller, since that needs the database.
pub fn clean(column: &Column, value: &Value) -> AppResult<Option<Value>> {
    if value.is_null() {
        return Ok(None);
    }
    let text = |max: usize| -> AppResult<Option<Value>> {
        let s = value.as_str().ok_or_else(|| bad(column, "expected text"))?;
        if s.chars().count() > max {
            return Err(bad(column, &format!("keep it under {max} characters")));
        }
        Ok((!s.trim().is_empty()).then(|| Value::String(s.to_string())))
    };
    match column.kind.as_str() {
        "text" => text(10_000),
        "url" => {
            let v = text(2_000)?;
            if let Some(Value::String(s)) = &v {
                let lower = s.to_lowercase();
                if !(lower.starts_with("http://")
                    || lower.starts_with("https://")
                    || lower.starts_with("mailto:"))
                {
                    return Err(bad(column, "a link starts with http://, https:// or mailto:"));
                }
            }
            Ok(v)
        }
        "number" | "currency" => {
            let n = match value {
                Value::Number(n) => n.as_f64(),
                Value::String(s) if s.trim().is_empty() => return Ok(None),
                Value::String(s) => s.trim().replace(',', "").parse::<f64>().ok(),
                _ => None,
            }
            .filter(|n| n.is_finite())
            .ok_or_else(|| bad(column, "expected a number"))?;
            Ok(Some(serde_json::json!(n)))
        }
        "checkbox" => match value {
            Value::Bool(b) => Ok(Some(Value::Bool(*b))),
            _ => Err(bad(column, "expected true or false")),
        },
        "date" => {
            let s = value.as_str().ok_or_else(|| bad(column, "expected a date"))?;
            if s.is_empty() {
                return Ok(None);
            }
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| bad(column, "expected YYYY-MM-DD"))?;
            Ok(Some(Value::String(s.to_string())))
        }
        "select" => {
            let s = value.as_str().ok_or_else(|| bad(column, "expected a choice"))?;
            if s.is_empty() {
                return Ok(None);
            }
            choice(column, s)?;
            Ok(Some(Value::String(s.to_string())))
        }
        "multi_select" => {
            let list = value.as_array().ok_or_else(|| bad(column, "expected a list of choices"))?;
            let mut ids: Vec<Value> = vec![];
            for v in list {
                let s = v.as_str().ok_or_else(|| bad(column, "expected a list of choices"))?;
                choice(column, s)?;
                if !ids.iter().any(|i| i == v) {
                    ids.push(v.clone());
                }
            }
            Ok((!ids.is_empty()).then_some(Value::Array(ids)))
        }
        "person" => {
            let s = value.as_str().ok_or_else(|| bad(column, "expected a person"))?;
            Ok((!s.is_empty()).then(|| Value::String(s.to_string())))
        }
        "relation" => Err(bad(column, "set relations with `relations`, not `values`")),
        _ => Err(bad(column, "unknown column type")),
    }
}

fn choice(column: &Column, id: &str) -> AppResult<()> {
    if column.options.choices.iter().any(|c| c.id == id) {
        Ok(())
    } else {
        Err(bad(column, "that choice isn't one of the column's options"))
    }
}

/// Turn a stored value into the best value for a column of another kind, for
/// when someone changes a column's type. `None` drops it.
pub fn convert(from: &Column, to: &Column, value: &Value) -> Option<Value> {
    let as_text = display(from, value);
    match to.kind.as_str() {
        "text" => (!as_text.is_empty()).then_some(Value::String(as_text)),
        "url" => (as_text.starts_with("http://") || as_text.starts_with("https://"))
            .then_some(Value::String(as_text)),
        "number" | "currency" => as_text.replace(',', "").parse::<f64>().ok().map(|n| serde_json::json!(n)),
        "checkbox" => match value {
            Value::Bool(b) => Some(Value::Bool(*b)),
            _ => Some(Value::Bool(matches!(as_text.to_lowercase().as_str(), "yes" | "true" | "1" | "x"))),
        },
        "date" => {
            chrono::NaiveDate::parse_from_str(&as_text, "%Y-%m-%d").ok().map(|_| Value::String(as_text))
        }
        "select" => {
            let first = as_text.split(", ").next().unwrap_or("");
            to.options
                .choices
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(first))
                .map(|c| Value::String(c.id.clone()))
        }
        "multi_select" => {
            let ids: Vec<Value> = as_text
                .split(", ")
                .filter_map(|n| to.options.choices.iter().find(|c| c.name.eq_ignore_ascii_case(n)))
                .map(|c| Value::String(c.id.clone()))
                .collect();
            (!ids.is_empty()).then_some(Value::Array(ids))
        }
        _ => None,
    }
}

/// A value as plain text: choice names, `Yes`/`No`, numbers without trailing zeros.
pub fn display(column: &Column, value: &Value) -> String {
    let name = |id: &str| {
        column.options.choices.iter().find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_default()
    };
    match (column.kind.as_str(), value) {
        (_, Value::Null) => String::new(),
        ("select", Value::String(id)) => name(id),
        ("multi_select", Value::Array(ids)) => ids
            .iter()
            .filter_map(Value::as_str)
            .map(name)
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        ("checkbox", Value::Bool(b)) => if *b { "Yes" } else { "No" }.into(),
        ("currency", Value::Number(n)) => format!("{:.2}", n.as_f64().unwrap_or(0.0)),
        (_, Value::Number(n)) => number(n.as_f64().unwrap_or(0.0)),
        (_, Value::String(s)) => s.clone(),
        (_, Value::Array(list)) => list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", "),
        (_, v) => v.to_string(),
    }
}

pub fn number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

fn is_empty(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(a)) => a.is_empty(),
        Some(Value::Bool(b)) => !b,
        _ => false,
    }
}

/// Check a filter before saving a view.
pub fn check_filter(f: &Filter, columns: &[Column]) -> AppResult<()> {
    if !columns.iter().any(|c| c.id == f.column_id) {
        return Err(AppError::BadRequest(
            "invalid_filter",
            "A filter names a column that isn't there.".into(),
        ));
    }
    if !FILTER_OPS.contains(&f.op.as_str()) {
        return Err(AppError::BadRequest(
            "invalid_filter",
            format!("A filter is one of {}.", FILTER_OPS.join(", ")),
        ));
    }
    Ok(())
}

/// Whether a cell passes a filter. Relations arrive as an array of target ids.
pub fn matches(f: &Filter, column: &Column, v: Option<&Value>) -> bool {
    let want = f.value.as_ref();
    match f.op.as_str() {
        "empty" => is_empty(v),
        "not_empty" => !is_empty(v),
        "is" | "is_not" => {
            let hit = match (v, want) {
                (_, None) | (_, Some(Value::Null)) => is_empty(v),
                (Some(Value::Array(list)), Some(w)) => list.contains(w),
                (Some(Value::Number(a)), Some(w)) => {
                    w.as_f64().or_else(|| w.as_str().and_then(|s| s.parse().ok())) == a.as_f64()
                }
                (Some(Value::String(a)), Some(Value::String(b))) => a.eq_ignore_ascii_case(b),
                (Some(a), Some(b)) => a == b,
                (None, Some(Value::Bool(false))) => true,
                (None, Some(_)) => false,
            };
            hit == (f.op == "is")
        }
        "contains" => {
            let needle = want.map(|w| match w {
                Value::String(s) => s.to_lowercase(),
                other => other.to_string(),
            });
            match (v, needle) {
                (_, None) => true,
                (Some(cell), Some(n)) => display(column, cell).to_lowercase().contains(&n),
                (None, Some(_)) => false,
            }
        }
        "gt" | "lt" => {
            let (Some(cell), Some(w)) = (v, want) else { return false };
            let ord = compare_values(cell, w);
            if f.op == "gt" { ord == Ordering::Greater } else { ord == Ordering::Less }
        }
        _ => true,
    }
}

fn compare_values(a: &Value, b: &Value) -> Ordering {
    let num = |v: &Value| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()));
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => match (a, b) {
            (Value::String(x), Value::String(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
            (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
            _ => a.to_string().cmp(&b.to_string()),
        },
    }
}

/// Order two cells for a sort. Empty cells always go last; choices sort in
/// the order the column lists them.
pub fn compare(sort: &Sort, column: &Column, a: Option<&Value>, b: Option<&Value>) -> Ordering {
    match (is_empty(a) && column.kind != "checkbox", is_empty(b) && column.kind != "checkbox") {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let null = Value::Null;
    let (a, b) = (a.unwrap_or(&null), b.unwrap_or(&null));
    let ord = match column.kind.as_str() {
        "select" => {
            let at = |v: &Value| {
                v.as_str()
                    .and_then(|id| column.options.choices.iter().position(|c| c.id == id))
                    .unwrap_or(usize::MAX)
            };
            at(a).cmp(&at(b))
        }
        "checkbox" => a.as_bool().unwrap_or(false).cmp(&b.as_bool().unwrap_or(false)),
        "multi_select" | "relation" => {
            display(column, a).to_lowercase().cmp(&display(column, b).to_lowercase())
        }
        _ => compare_values(a, b),
    };
    if sort.desc { ord.reverse() } else { ord }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::databases::{Choice, ColumnOptions};
    use serde_json::json;

    fn col(kind: &str) -> Column {
        Column {
            id: "c".into(),
            database_id: "d".into(),
            name: "Status".into(),
            kind: kind.into(),
            options: ColumnOptions {
                choices: vec![
                    Choice { id: "a".into(), name: "To order".into(), color: None },
                    Choice { id: "b".into(), name: "Ordered".into(), color: None },
                ],
                ..Default::default()
            },
            is_title: false,
            position: "a".into(),
        }
    }

    #[test]
    fn cleans_values() {
        assert_eq!(clean(&col("number"), &json!("1,200.5")).unwrap(), Some(json!(1200.5)));
        assert_eq!(clean(&col("text"), &json!("  ")).unwrap(), None);
        assert!(clean(&col("select"), &json!("zzz")).is_err());
        assert_eq!(clean(&col("multi_select"), &json!(["a", "a", "b"])).unwrap(), Some(json!(["a", "b"])));
        assert!(clean(&col("date"), &json!("6/11/2026")).is_err());
        assert!(clean(&col("url"), &json!("javascript:alert(1)")).is_err());
    }

    #[test]
    fn filters_and_sorts() {
        let c = col("select");
        let f = |op: &str, v: Value| Filter { column_id: "c".into(), op: op.into(), value: Some(v) };
        assert!(matches(&f("is", json!("a")), &c, Some(&json!("a"))));
        assert!(matches(&f("is_not", json!("a")), &c, None));
        assert!(matches(&f("contains", json!("order")), &c, Some(&json!("b"))));
        assert!(matches(&f("gt", json!(10)), &col("number"), Some(&json!(42.0))));
        let s = Sort { column_id: "c".into(), desc: false };
        assert_eq!(compare(&s, &c, Some(&json!("b")), Some(&json!("a"))), Ordering::Greater);
        assert_eq!(compare(&s, &c, None, Some(&json!("a"))), Ordering::Greater);
        assert_eq!(display(&col("multi_select"), &json!(["a", "b"])), "To order, Ordered");
        assert_eq!(display(&col("currency"), &json!(9.9)), "9.90");
    }
}
