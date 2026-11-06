//! MCP access to project databases: read a view as a markdown table, add a row
//! with values given by column name.

use std::collections::HashMap;

use serde_json::Value;
use sqlx::SqliteConnection;

use super::{bad, find_project};
use crate::{
    auth::User,
    databases::{self, Column, CreateRowRequest, DatabaseRef, RelationTarget},
    error::{AppError, AppResult},
    projects::editable_project,
};

/// A database by id, or by name (any case) among those you can see, in
/// `project` when given.
pub(super) async fn find_database(
    conn: &mut SqliteConnection,
    user: &User,
    key: &str,
    project: Option<&str>,
) -> AppResult<DatabaseRef> {
    let key = key.trim();
    let all = databases::all_refs(conn, &user.id).await?;
    if let Some(d) = all.iter().find(|d| d.id == key) {
        return Ok(d.clone());
    }
    let project_id = match project {
        Some(p) => Some(find_project(conn, user, p).await?.id),
        None => None,
    };
    let hits: Vec<&DatabaseRef> = all
        .iter()
        .filter(|d| {
            d.name.eq_ignore_ascii_case(key) && project_id.as_ref().is_none_or(|p| &d.project_id == p)
        })
        .collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => {
            let names: Vec<String> = all
                .iter()
                .filter(|d| project_id.as_ref().is_none_or(|p| &d.project_id == p))
                .map(|d| format!("{} ({})", d.name, d.project_title))
                .collect();
            Err(bad(
                "unknown_database",
                if names.is_empty() {
                    "No databases there yet.".to_string()
                } else {
                    format!("No database called {key}. There are: {}.", names.join(", "))
                },
            ))
        }
        many => Err(bad(
            "ambiguous_database",
            format!(
                "More than one database is called {key}, in {}. Say which project.",
                many.iter().map(|d| d.project_title.as_str()).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}

fn cell(column: &Column, row: &databases::Row) -> String {
    let s = if column.kind == "relation" {
        row.relations
            .get(&column.id)
            .map(|l| l.iter().map(|r| r.title.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default()
    } else {
        row.values.get(&column.id).map(|v| databases::values::display(column, v)).unwrap_or_default()
    };
    s.replace('|', "\\|").replace('\n', " ")
}

/// Rows as a markdown table, through a view if named.
pub(super) async fn query(
    conn: &mut SqliteConnection,
    user: &User,
    database: &str,
    project: Option<&str>,
    view: Option<&str>,
    search: Option<&str>,
    limit: Option<u32>,
) -> AppResult<String> {
    let d = find_database(conn, user, database, project).await?;
    let v = match view {
        Some(name) => Some(
            d.views
                .iter()
                .find(|v| v.name.eq_ignore_ascii_case(name.trim()) || v.id == name)
                .cloned()
                .ok_or_else(|| {
                    bad(
                        "unknown_view",
                        format!(
                            "{} has no view called {name}. Its views: {}.",
                            d.name,
                            d.views.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", ")
                        ),
                    )
                })?,
        ),
        None => None,
    };
    let columns = databases::columns_of(conn, &d.id).await?;
    let rows = databases::query_rows(conn, &user.id, &d.id, v.as_ref(), search).await?;
    let shown: Vec<&Column> = columns
        .iter()
        .filter(|c| v.as_ref().is_none_or(|v| c.is_title || !v.config.hidden.contains(&c.id)))
        .collect();
    let limit = limit.unwrap_or(50).clamp(1, 200) as usize;
    let mut out = format!("# {} · {}\n\n", d.name, d.project_title);
    if let Some(v) = &v {
        out.push_str(&format!("View: {} ({})\n", v.name, v.kind));
    }
    out.push_str(&format!(
        "{} row{}{}\n\n",
        rows.len(),
        if rows.len() == 1 { "" } else { "s" },
        if rows.len() > limit { format!(", first {limit} shown") } else { String::new() }
    ));
    out.push_str(&format!(
        "Columns: {}\n\n",
        columns
            .iter()
            .map(|c| {
                let extra = match c.kind.as_str() {
                    "select" | "multi_select" => format!(
                        ": {}",
                        c.options.choices.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(" / ")
                    ),
                    "currency" => format!(" {}", c.options.currency.as_deref().unwrap_or("")),
                    "relation" => format!(" to {}s", c.options.target.as_deref().unwrap_or("")),
                    _ => String::new(),
                };
                format!("{} ({}{extra})", c.name, c.kind)
            })
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if rows.is_empty() {
        out.push_str("No rows.\n");
        return Ok(out);
    }
    out.push_str(&format!(
        "| {} |\n",
        shown.iter().map(|c| c.name.replace('|', "\\|")).collect::<Vec<_>>().join(" | ")
    ));
    out.push_str(&format!("|{}\n", " --- |".repeat(shown.len())));
    for row in rows.iter().take(limit) {
        out.push_str(&format!(
            "| {} |\n",
            shown.iter().map(|c| cell(c, row)).collect::<Vec<_>>().join(" | ")
        ));
    }
    // Totals for number and currency columns, which is what people usually ask.
    let totals: Vec<String> = shown
        .iter()
        .filter(|c| matches!(c.kind.as_str(), "number" | "currency"))
        .filter_map(|c| {
            let sum: f64 = rows.iter().filter_map(|r| r.values.get(&c.id).and_then(Value::as_f64)).sum();
            (sum != 0.0).then(|| {
                let shown = if c.kind == "currency" {
                    format!("{sum:.2} {}", c.options.currency.as_deref().unwrap_or(""))
                } else {
                    databases::values::number(sum)
                };
                format!("{} {}", c.name, shown.trim())
            })
        })
        .collect();
    if !totals.is_empty() {
        out.push_str(&format!("\nTotals across all {} rows: {}\n", rows.len(), totals.join(", ")));
    }
    Ok(out)
}

/// Turn a value given by name into what the column stores.
async fn value_for(
    conn: &mut SqliteConnection,
    project_id: &str,
    column: &Column,
    v: &Value,
) -> AppResult<Value> {
    let as_list = |v: &Value| -> Vec<String> {
        match v {
            Value::Array(a) => {
                a.iter().map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).collect()
            }
            Value::String(s) => {
                s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
            }
            other => vec![other.to_string()],
        }
    };
    let choice = |name: &str| -> AppResult<Value> {
        column
            .options
            .choices
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name.trim()) || c.id == name)
            .map(|c| Value::String(c.id.clone()))
            .ok_or_else(|| {
                bad(
                    "invalid_value",
                    format!(
                        "{} is one of: {}.",
                        column.name,
                        column.options.choices.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                )
            })
    };
    Ok(match column.kind.as_str() {
        "select" => match v {
            Value::Null => Value::Null,
            _ => choice(&as_list(v).join(", "))?,
        },
        "multi_select" => Value::Array(as_list(v).iter().map(|n| choice(n)).collect::<AppResult<Vec<_>>>()?),
        "checkbox" => match v {
            Value::Bool(b) => Value::Bool(*b),
            Value::String(s) => {
                Value::Bool(matches!(s.to_lowercase().as_str(), "yes" | "true" | "x" | "done"))
            }
            _ => Value::Bool(false),
        },
        "person" => {
            let who = v.as_str().unwrap_or("").trim().to_string();
            let id: Option<String> = sqlx::query_scalar(
                "SELECT u.id FROM users u JOIN project_access a ON a.user_id = u.id AND a.project_id = ?
                 WHERE u.id = ?2 OR u.name = ?2 COLLATE NOCASE OR u.email = ?2 COLLATE NOCASE LIMIT 1",
            )
            .bind(project_id)
            .bind(&who)
            .fetch_optional(&mut *conn)
            .await?;
            Value::String(id.ok_or_else(|| {
                bad("invalid_value", format!("{}: nobody called {who} can see this project.", column.name))
            })?)
        }
        _ => v.clone(),
    })
}

/// Links for a relation column, from titles (or `T-12` for tasks).
async fn targets_for(
    conn: &mut SqliteConnection,
    user: &User,
    project_id: &str,
    column: &Column,
    v: &Value,
) -> AppResult<Vec<RelationTarget>> {
    let names: Vec<String> = match v {
        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Value::String(s) => s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect(),
        _ => vec![],
    };
    let target = column.options.target.clone().unwrap_or_default();
    let mut out = vec![];
    for name in names {
        let id: Option<String> = match target.as_str() {
            "task" => {
                let number = name.trim().strip_prefix("T-").or(name.trim().strip_prefix("t-")).and_then(|n| n.parse::<i64>().ok());
                sqlx::query_scalar(
                    "SELECT t.id FROM tasks t JOIN project_access a ON a.project_id = t.project_id AND a.user_id = ?
                     WHERE t.id = ?2 OR (t.project_id = ?3 AND (t.number = ?4 OR t.title = ?2 COLLATE NOCASE))
                     ORDER BY t.project_id = ?3 DESC LIMIT 1",
                )
                .bind(&user.id)
                .bind(&name)
                .bind(project_id)
                .bind(number.unwrap_or(-1))
                .fetch_optional(&mut *conn)
                .await?
            }
            "note" => sqlx::query_scalar(
                "SELECT n.id FROM notes n JOIN note_access a ON a.note_id = n.id AND a.user_id = ?
                 WHERE n.id = ?2 OR n.title = ?2 COLLATE NOCASE ORDER BY n.user_id = ?1 DESC LIMIT 1",
            )
            .bind(&user.id)
            .bind(&name)
            .fetch_optional(&mut *conn)
            .await?,
            "project" => Some(find_project(conn, user, &name).await?.id),
            _ => sqlx::query_scalar(
                "SELECT r.id FROM db_rows r JOIN db_columns c ON c.database_id = r.database_id AND c.is_title = 1
                 WHERE r.database_id = ? AND (r.id = ?2 OR json_extract(r.data, '$.\"' || c.id || '\"') = ?2 COLLATE NOCASE)
                 LIMIT 1",
            )
            .bind(column.options.target_database_id.as_deref().unwrap_or(""))
            .bind(&name)
            .fetch_optional(&mut *conn)
            .await?,
        };
        let id = id.ok_or_else(|| {
            bad("invalid_value", format!("{}: can't find the {target} {name}.", column.name))
        })?;
        out.push(RelationTarget { kind: target.clone(), id });
    }
    Ok(out)
}

/// Add a row from values keyed by column name.
pub(super) async fn add_row(
    conn: &mut SqliteConnection,
    user: &User,
    database: &str,
    project: Option<&str>,
    input: &HashMap<String, Value>,
) -> AppResult<String> {
    let d = find_database(conn, user, database, project).await?;
    editable_project(conn, &user.id, &d.project_id).await?;
    let columns = databases::columns_of(conn, &d.id).await?;
    let mut req = CreateRowRequest::default();
    for (name, v) in input {
        let column = columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name.trim()) || c.id == *name)
            .ok_or_else(|| {
                bad(
                    "invalid_column",
                    format!(
                        "{} has no column called {name}. Its columns: {}.",
                        d.name,
                        columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                )
            })?;
        if column.kind == "relation" {
            let targets = targets_for(conn, user, &d.project_id, column, v).await?;
            req.relations.insert(column.id.clone(), targets);
        } else {
            let value = value_for(conn, &d.project_id, column, v).await?;
            req.values.insert(column.id.clone(), value);
        }
    }
    let title = columns.iter().find(|c| c.is_title);
    let name =
        title.and_then(|t| req.values.get(&t.id)).and_then(Value::as_str).unwrap_or("Untitled").to_string();
    databases::insert_row(conn, &user.id, &d.project_id, &d.id, &req).await.map_err(|e| match e {
        AppError::BadRequest(code, msg) => bad(code, msg),
        other => other,
    })?;
    Ok(format!("Added “{name}” to {} in {}.", d.name, d.project_title))
}
