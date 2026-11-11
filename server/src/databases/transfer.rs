//! Database export (a CSV plus one markdown file per row, like Notion's
//! export) and CSV import (from Notion, a spreadsheet, or our own export).

use std::io::{Cursor, Write};

use anyhow::Context;
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Map, Value};
use sqlx::SqliteConnection;
use utoipa::IntoParams;

use super::{
    Choice, Column, ColumnOptions, CreateRowRequest, DatabaseDetail, MAX_ROWS, csv, detail, insert_column,
    insert_database, insert_row, query_rows, values, visible_database,
};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    notes::transfer::attachment,
    projects::editable_project,
};

pub(super) const MAX_CSV_BYTES: usize = 20 * 1024 * 1024;

/// A cell as plain text, relations as their titles.
fn cell_text(column: &Column, row: &super::Row) -> String {
    if column.kind == "relation" {
        return row
            .relations
            .get(&column.id)
            .map(|l| l.iter().map(|r| r.title.as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
    }
    row.values.get(&column.id).map(|v| values::display(column, v)).unwrap_or_default()
}

/// A file name without characters that trouble file systems.
fn file_name(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').to_string();
    if trimmed.is_empty() { "Untitled".into() } else { trimmed.chars().take(120).collect() }
}

fn yaml_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// A database as a zip: `<name>.csv` and `<name>/<row>.md` with the row's
/// values in front matter.
#[utoipa::path(get, path = "/databases/{id}/export", tag = "databases", params(("id" = String, Path)),
    responses((status = 200, content_type = "application/zip", body = Vec<u8>), (status = 404, body = crate::error::ErrorBody)))]
pub(super) async fn export_database(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let mut conn = state.db.acquire().await?;
    let (_, project) = visible_database(&mut conn, &user.id, &id).await?;
    let d = detail(&mut conn, &id, &project).await?;
    let rows = query_rows(&mut conn, &user.id, &id, None, None).await?;
    let name = file_name(&d.database.name);

    let mut text = csv::line(&d.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>());
    for row in &rows {
        text.push_str(&csv::line(&d.columns.iter().map(|c| cell_text(c, row)).collect::<Vec<_>>()));
    }

    let bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        zip.start_file(format!("{name}.csv"), opts)?;
        zip.write_all(b"\xef\xbb\xbf")?;
        zip.write_all(text.as_bytes())?;
        let title = d.columns.iter().find(|c| c.is_title);
        let mut used: Vec<String> = vec![];
        for row in &rows {
            let mut base = title.map(|t| file_name(&cell_text(t, row))).unwrap_or_else(|| "Untitled".into());
            let mut n = 2;
            let original = base.clone();
            while used.contains(&base.to_lowercase()) {
                base = format!("{original} {n}");
                n += 1;
            }
            used.push(base.to_lowercase());
            let mut md = String::from("---\n");
            for c in d.columns.iter().filter(|c| !c.is_title) {
                let v = cell_text(c, row);
                if v.is_empty() {
                    continue;
                }
                let value = match (c.kind.as_str(), row.values.get(&c.id)) {
                    ("number" | "currency", Some(Value::Number(n))) => n.to_string(),
                    ("checkbox", Some(Value::Bool(b))) => b.to_string(),
                    ("multi_select" | "relation", _) => {
                        format!("[{}]", v.split(", ").map(yaml_string).collect::<Vec<_>>().join(", "))
                    }
                    _ => yaml_string(&v),
                };
                md.push_str(&format!("{}: {value}\n", yaml_string(&c.name)));
            }
            md.push_str("---\n");
            zip.start_file(format!("{name}/{base}.md"), opts)?;
            zip.write_all(md.as_bytes())?;
        }
        Ok(zip.finish()?.into_inner())
    })
    .await
    .context("exporting database")??;

    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, attachment(&format!("{}.zip", file_name(&d.database.name)))),
        ],
        bytes,
    )
        .into_response())
}

#[derive(Deserialize, IntoParams)]
pub struct ImportQuery {
    /// The new database's name. Defaults to "Imported".
    pub name: Option<String>,
}

/// Make a database from a CSV file, such as a Notion database export. The
/// first column becomes the name; the others get a type guessed from their
/// values.
#[utoipa::path(post, path = "/projects/{id}/databases/import", tag = "databases", params(("id" = String, Path), ImportQuery),
    request_body(content = String, content_type = "text/csv"),
    responses((status = 201, body = DatabaseDetail), (status = 400, body = crate::error::ErrorBody)))]
pub(super) async fn import_database(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<ImportQuery>,
    body: Bytes,
) -> AppResult<(StatusCode, Json<DatabaseDetail>)> {
    if body.len() > MAX_CSV_BYTES {
        return Err(AppError::BadRequest("import_too_large", "That file is over 20 MB.".into()));
    }
    let text = String::from_utf8(body.to_vec())
        .map_err(|_| AppError::BadRequest("invalid_csv", "That file isn't UTF-8 text.".into()))?;
    let rows = csv::parse(&text);
    let mut tx = state.db.begin().await?;
    let project = editable_project(&mut tx, &user.id, &id).await?;
    let name = q.name.as_deref().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Imported");
    let name: String = name.chars().take(60).collect();
    let db = import_rows(&mut tx, &user.id, &id, &name, &rows).await?;
    let out = detail(&mut tx, &db, &project).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(out)))
}

/// Create a database from parsed CSV rows, header first.
pub(crate) async fn import_rows(
    conn: &mut SqliteConnection,
    user_id: &str,
    project_id: &str,
    name: &str,
    rows: &[Vec<String>],
) -> AppResult<String> {
    let Some((header, body)) = rows.split_first() else {
        return Err(AppError::BadRequest("invalid_csv", "That CSV is empty.".into()));
    };
    if header.iter().all(|h| h.trim().is_empty()) {
        return Err(AppError::BadRequest("invalid_csv", "The first line should name the columns.".into()));
    }
    if body.len() as i64 > MAX_ROWS {
        return Err(AppError::BadRequest("too_many_rows", "A database can have up to 10,000 rows.".into()));
    }
    // Our own exports escape formula-like cells; take that back off.
    let header: Vec<String> = header.iter().take(60).map(|h| csv::unescape_formula(h).to_string()).collect();
    let db = insert_database(conn, project_id, name).await?;

    let mut columns: Vec<Column> = vec![];
    let mut seen: Vec<String> = vec![];
    for (i, raw) in header.iter().enumerate() {
        let mut col_name: String = raw.trim().chars().take(60).collect();
        if col_name.is_empty() {
            col_name = format!("Column {}", i + 1);
        }
        let base = col_name.clone();
        let mut n = 2;
        while seen.contains(&col_name.to_lowercase()) {
            col_name = format!("{base} {n}");
            n += 1;
        }
        seen.push(col_name.to_lowercase());
        let cells: Vec<&str> =
            body.iter().map(|r| r.get(i).map(|s| csv::unescape_formula(s)).unwrap_or("")).collect();
        let (kind, options) = if i == 0 {
            ("text", ColumnOptions::default())
        } else {
            match csv::guess(&cells) {
                csv::Guess::Number => ("number", ColumnOptions::default()),
                csv::Guess::Checkbox => ("checkbox", ColumnOptions::default()),
                csv::Guess::Date => ("date", ColumnOptions::default()),
                csv::Guess::Url => ("url", ColumnOptions::default()),
                g @ (csv::Guess::Select | csv::Guess::MultiSelect) => {
                    let mut names: Vec<String> = vec![];
                    for c in &cells {
                        for part in c.split(", ").map(str::trim).filter(|p| !p.is_empty()) {
                            if !names.iter().any(|n| n.eq_ignore_ascii_case(part)) {
                                names.push(part.to_string());
                            }
                        }
                    }
                    let choices = names
                        .into_iter()
                        .enumerate()
                        .map(|(i, name)| Choice {
                            id: crate::time::new_id(),
                            name,
                            color: Some(format!("slot-{}", i % 8 + 1)),
                        })
                        .collect();
                    let kind = if g == csv::Guess::Select { "select" } else { "multi_select" };
                    (kind, ColumnOptions { choices, ..Default::default() })
                }
                csv::Guess::Text => ("text", ColumnOptions::default()),
            }
        };
        columns.push(insert_column(conn, &db, &col_name, kind, &options, i == 0).await?);
    }

    for r in body {
        let mut values = Map::new();
        for (i, column) in columns.iter().enumerate() {
            let raw = r.get(i).map(|s| csv::unescape_formula(s).trim()).unwrap_or("");
            if raw.is_empty() {
                continue;
            }
            let v = match column.kind.as_str() {
                "number" => csv::number(raw).map(|n| serde_json::json!(n)),
                "checkbox" => csv::checkbox(raw).map(Value::Bool),
                "date" => csv::date(raw).map(Value::String),
                "select" => column
                    .options
                    .choices
                    .iter()
                    .find(|c| c.name.eq_ignore_ascii_case(raw))
                    .map(|c| Value::String(c.id.clone())),
                "multi_select" => Some(Value::Array(
                    raw.split(", ")
                        .filter_map(|p| {
                            column.options.choices.iter().find(|c| c.name.eq_ignore_ascii_case(p.trim()))
                        })
                        .map(|c| Value::String(c.id.clone()))
                        .collect(),
                )),
                _ => Some(Value::String(raw.chars().take(10_000).collect())),
            };
            if let Some(v) = v {
                values.insert(column.id.clone(), v);
            }
        }
        insert_row(conn, user_id, project_id, &db, &CreateRowRequest { values, ..Default::default() })
            .await?;
    }
    Ok(db)
}
