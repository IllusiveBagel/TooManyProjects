//! Moving whole vaults in and out as zip files: export everything, or import
//! an Obsidian vault or a Notion "Markdown & CSV" export.

use std::{
    io::{Cursor, Read, Write},
    path::Path,
};

use anyhow::Context;
use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::header,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::{markdown, vault};
use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    time,
};

/// Biggest zip accepted by import.
pub const MAX_IMPORT_BYTES: usize = 200 * 1024 * 1024;
/// Most bytes an import may unpack, so a small zip cannot fill the disk.
const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
/// Largest attachment (image, PDF…) kept from an import.
const MAX_ATTACHMENT_BYTES: u64 = 50 * 1024 * 1024;

/// A `Content-Disposition` value that offers `name` as a download, with a
/// plain ASCII fallback and the full name in RFC 5987 form.
pub fn attachment(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' })
        .collect();
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

#[derive(Deserialize, ToSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ImportSource {
    Obsidian,
    Notion,
}

#[derive(Deserialize, IntoParams)]
pub struct ImportQuery {
    /// Where the zip came from: `obsidian` (a zipped vault folder) or
    /// `notion` (Export, Markdown & CSV).
    #[param(inline)]
    pub source: ImportSource,
    /// Folder to put everything under, such as `Imported`. Empty means the
    /// vault root.
    pub folder: Option<String>,
}

#[derive(Serialize, ToSchema, Debug)]
pub struct ImportResult {
    /// Markdown notes written to the vault.
    pub notes: usize,
    /// Other files kept, such as images and PDFs.
    pub attachments: usize,
    /// Entries left out: settings folders, databases (CSV), oversized files.
    pub skipped: usize,
    /// Notes that were renamed because a note with that name already existed.
    pub renamed: usize,
}

/// Every file in your vault, notes and attachments, as one zip.
#[utoipa::path(get, path = "/notes/export", tag = "notes",
    responses((status = 200, content_type = "application/zip", body = Vec<u8>)))]
pub(super) async fn export_vault(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> AppResult<Response> {
    let root = vault::root(&state.config.data_dir, &user.id);
    let bytes = tokio::task::spawn_blocking(move || zip_dir(&root)).await.context("exporting vault")??;
    let name = format!("TooManyProjects notes {}.zip", &time::now()[..10]);
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip".to_string()),
            (header::CONTENT_DISPOSITION, attachment(&name)),
        ],
        bytes,
    )
        .into_response())
}

fn zip_dir(root: &Path) -> anyhow::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    let mut stack = vec![root.to_path_buf()];
    let mut files = vec![];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // Hidden files and the half-written temporaries of an atomic save.
            if name.starts_with('.') || name.ends_with(".tmp") {
                continue;
            }
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(t) if t.is_file() => files.push(e.path()),
                _ => {}
            }
        }
    }
    files.sort();
    for path in files {
        let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        zip.start_file(rel, opts)?;
        let mut f = std::fs::File::open(&path)?;
        std::io::copy(&mut f, &mut zip)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// Import a zip of markdown files. Notes keep their folders; a note whose
/// path is taken gets ` 2` added. Hidden folders such as `.obsidian` and
/// Notion's CSV databases are skipped.
#[utoipa::path(post, path = "/notes/import", tag = "notes", params(ImportQuery),
    request_body(content = Vec<u8>, content_type = "application/zip"),
    responses((status = 200, body = ImportResult), (status = 400, body = crate::error::ErrorBody)))]
pub(super) async fn import_vault(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<ImportQuery>,
    body: Bytes,
) -> AppResult<Json<ImportResult>> {
    if body.len() > MAX_IMPORT_BYTES {
        return Err(AppError::BadRequest("import_too_large", "That zip is over 200 MB.".into()));
    }
    let root = vault::root(&state.config.data_dir, &user.id);
    let folder = vault::clean_folder(q.folder.as_deref().unwrap_or(""));
    let source = q.source;
    let unpack_root = root.clone();
    let result = tokio::task::spawn_blocking(move || unpack(&body, &unpack_root, &folder, source))
        .await
        .context("importing notes")??;
    vault::scan(&state.db, &state.config.data_dir, &user.id).await?;
    Ok(Json(result))
}

fn bad_zip() -> AppError {
    AppError::BadRequest("invalid_zip", "That file is not a zip we can read.".into())
}

struct Entry {
    /// Path inside the zip, cleaned, with Notion ids stripped.
    rel: String,
    index: usize,
}

fn unpack(bytes: &[u8], root: &Path, folder: &str, source: ImportSource) -> AppResult<ImportResult> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| bad_zip())?;
    if archive.len() > MAX_ENTRIES {
        return Err(AppError::BadRequest("import_too_large", "That zip holds over 20,000 files.".into()));
    }
    let mut result = ImportResult { notes: 0, attachments: 0, skipped: 0, renamed: 0 };

    let mut entries = vec![];
    for index in 0..archive.len() {
        let file = archive.by_index_raw(index).map_err(|_| bad_zip())?;
        if file.is_dir() {
            continue;
        }
        let raw = file.name().map_err(|_| bad_zip())?.replace('\\', "/");
        if raw.starts_with("__MACOSX/") || raw.split('/').any(|s| s.starts_with('.')) {
            result.skipped += 1;
            continue;
        }
        let renamed = match source {
            ImportSource::Notion => {
                raw.split('/').map(markdown::strip_notion_id).collect::<Vec<_>>().join("/")
            }
            ImportSource::Obsidian => raw,
        };
        let lower = renamed.to_lowercase();
        if lower.ends_with(".csv") || lower.ends_with("_all.csv") {
            result.skipped += 1;
            continue;
        }
        match vault::clean_rel(&renamed) {
            Some(rel) => entries.push(Entry { rel, index }),
            None => result.skipped += 1,
        }
    }

    // A zip of a folder puts everything under that folder's name; drop it.
    let shared = entries.first().and_then(|e| e.rel.split_once('/')).map(|(top, _)| format!("{top}/"));
    if let Some(top) = shared
        && entries.iter().all(|e| e.rel.starts_with(&top))
    {
        for e in &mut entries {
            e.rel = e.rel[top.len()..].to_string();
        }
    }

    let mut unpacked = 0u64;
    for e in entries {
        let mut file = archive.by_index(e.index).map_err(|_| bad_zip())?;
        let is_note = e.rel.to_lowercase().ends_with(".md");
        let limit = if is_note { vault::MAX_NOTE_BYTES } else { MAX_ATTACHMENT_BYTES };
        if file.size() > limit || unpacked + file.size() > MAX_UNPACKED_BYTES {
            result.skipped += 1;
            continue;
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        // `take` guards against a header that lies about the size.
        (&mut file).take(limit + 1).read_to_end(&mut data).map_err(|_| bad_zip())?;
        if data.len() as u64 > limit {
            result.skipped += 1;
            continue;
        }
        unpacked += data.len() as u64;

        let rel = if folder.is_empty() { e.rel.clone() } else { format!("{folder}/{}", e.rel) };
        let (dir, name) = match rel.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), rel.clone()),
        };
        if is_note {
            let mut text = String::from_utf8_lossy(&data).into_owned();
            if source == ImportSource::Notion {
                text = notion_note(&text);
            }
            let stem = name[..name.len() - 3].to_string();
            let target = vault::free_path(root, &dir, &markdown::file_stem(&stem), None);
            if target != rel {
                result.renamed += 1;
            }
            write(root, &target, text.as_bytes())?;
            result.notes += 1;
        } else {
            let target = root.join(&rel);
            if target.exists() {
                // Same name: keep what is there. Attachments are found by name.
                result.skipped += 1;
                continue;
            }
            write(root, &rel, &data)?;
            result.attachments += 1;
        }
    }
    Ok(result)
}

fn write(root: &Path, rel: &str, data: &[u8]) -> AppResult<()> {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).context("creating vault folder")?;
    }
    let mut f = std::fs::File::create(&path).context("writing imported file")?;
    f.write_all(data).context("writing imported file")?;
    Ok(())
}

/// Notion pages start with `# Title` (the file name already says it) and link
/// to each other with markdown links; attachments sit in folders named after
/// the page, with the same ids.
fn notion_note(text: &str) -> String {
    let body = markdown::notion_links_to_wiki(text);
    let body = strip_ids_in_links(&body);
    let mut lines = body.lines();
    let first = body.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if first.starts_with("# ") {
        // Drop the title line and the blank lines around it.
        let mut out: Vec<&str> = lines.by_ref().skip_while(|l| l.trim().is_empty()).skip(1).collect();
        while out.first().is_some_and(|l| l.trim().is_empty()) {
            out.remove(0);
        }
        let mut s = out.join("\n");
        s.push('\n');
        return s;
    }
    body
}

/// `![x](Page%20<id>/image.png)` becomes `![x](Page/image.png)`, matching
/// the folders the import wrote.
fn strip_ids_in_links(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("](") {
        out.push_str(&rest[..start + 2]);
        rest = &rest[start + 2..];
        let Some(close) = rest.find(')') else { break };
        let target = &rest[..close];
        if target.contains("://") || target.starts_with('#') {
            out.push_str(target);
        } else {
            let fixed = target
                .split('/')
                .map(|seg| markdown::strip_notion_id(&seg.replace("%20", " ")).replace(' ', "%20"))
                .collect::<Vec<_>>()
                .join("/");
            out.push_str(&fixed);
        }
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition_is_safe() {
        assert_eq!(
            attachment("Brakes \"v2\" é.md"),
            "attachment; filename=\"Brakes _v2_ _.md\"; filename*=UTF-8''Brakes%20%22v2%22%20%C3%A9.md"
        );
    }

    #[test]
    fn notion_titles_and_links() {
        let id = "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d";
        let text = format!(
            "# Car\n\nSee [Parts](Parts%20list%20{id}.md) and ![img](Car%20{id}/photo.png) on [site](https://x.com/a%20b).\n"
        );
        assert_eq!(
            notion_note(&text),
            "See [[Parts list]] and ![img](Car/photo.png) on [site](https://x.com/a%20b).\n"
        );
    }
}
