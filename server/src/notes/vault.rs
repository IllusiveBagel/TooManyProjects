//! The vault on disk and the SQLite index of it.
//!
//! Each person has a folder, `<data_dir>/vaults/<user id>/`, laid out the way
//! Obsidian expects. The files are the source of truth: the index is rebuilt
//! from them by [`scan`], which runs at start-up, every few seconds in the
//! background, and on request. Writes made through the API update the file
//! first and then index it straight away.

use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::Context;
use sqlx::{SqliteConnection, SqlitePool};

use super::markdown::{self, FrontMatter};
use crate::{
    error::{AppError, AppResult},
    time,
};

pub const INBOX: &str = "Inbox";
pub const KNOWLEDGE: &str = "Knowledge";
pub const PROJECTS: &str = "Projects";
/// Largest note the API reads or writes.
pub const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;

pub fn root(data_dir: &Path, user_id: &str) -> PathBuf {
    data_dir.join("vaults").join(user_id)
}

/// A relative vault path made safe: forward slashes, no `..`, no absolute
/// parts, no hidden segments, each segment a clean file name.
pub fn clean_rel(path: &str) -> Option<String> {
    let mut parts = vec![];
    for part in Path::new(&path.replace('\\', "/")).components() {
        match part {
            Component::Normal(p) => {
                let s = p.to_string_lossy();
                if s.starts_with('.') {
                    return None;
                }
                parts.push(s.to_string());
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// A folder path chosen by a person: each segment run through `file_stem`.
pub fn clean_folder(folder: &str) -> String {
    folder
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .map(markdown::file_stem)
        .collect::<Vec<_>>()
        .join("/")
}

/// `folder/stem.md`, with ` 2`, ` 3`… added until no file has that name.
/// `except` is a path the caller already owns (a rename to itself).
pub fn free_path(root: &Path, folder: &str, stem: &str, except: Option<&str>) -> String {
    let join = |name: &str| if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") };
    for n in 1.. {
        let name = if n == 1 { format!("{stem}.md") } else { format!("{stem} {n}.md") };
        let rel = join(&name);
        if Some(rel.as_str()) == except || !root.join(&rel).exists() {
            return rel;
        }
    }
    unreachable!()
}

/// Writes a file atomically: a temporary file in the same folder, then a rename.
pub async fn write_file(root: &Path, rel: &str, text: &str) -> AppResult<()> {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await.context("creating vault folder")?;
    }
    let tmp = path.with_extension(format!("md.{}.tmp", time::new_id()));
    tokio::fs::write(&tmp, text).await.context("writing note")?;
    tokio::fs::rename(&tmp, &path).await.context("saving note")?;
    Ok(())
}

pub async fn read_file(root: &Path, rel: &str) -> AppResult<String> {
    let path = root.join(rel);
    let meta = tokio::fs::metadata(&path).await.map_err(|_| AppError::NotFound)?;
    if meta.len() > MAX_NOTE_BYTES {
        return Err(AppError::BadRequest("note_too_large", "That note is over 2 MB.".into()));
    }
    let bytes = tokio::fs::read(&path).await.map_err(|_| AppError::NotFound)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Removes empty folders between `rel`'s folder and the vault root.
pub async fn prune_empty_dirs(root: &Path, rel: &str) {
    let mut dir = root.join(rel);
    while dir.pop() && dir != root && dir.starts_with(root) {
        if tokio::fs::remove_dir(&dir).await.is_err() {
            break;
        }
    }
}

fn stat(path: &Path) -> Option<(i64, i64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64;
    Some((meta.len() as i64, mtime))
}

fn mtime_rfc3339(mtime_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(mtime_ms).map(time::format).unwrap_or_else(time::now)
}

/// Every `.md` file under the vault, as relative paths. Hidden folders
/// (`.obsidian`, `.git`, `.trash`) are skipped.
fn walk(root: &Path) -> Vec<String> {
    let mut out = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(path),
                Ok(t) if t.is_file() && name.to_lowercase().ends_with(".md") => {
                    if let Ok(rel) = path.strip_prefix(root) {
                        out.push(rel.to_string_lossy().replace('\\', "/"));
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// Brings the index in line with the files: new and changed files are read,
/// missing ones dropped. Returns how many files were (re)indexed.
pub async fn scan(db: &SqlitePool, data_dir: &Path, user_id: &str) -> AppResult<usize> {
    let root = root(data_dir, user_id);
    let walk_root = root.clone();
    let files = tokio::task::spawn_blocking(move || {
        walk(&walk_root)
            .into_iter()
            .filter_map(|rel| stat(&walk_root.join(&rel)).map(|s| (rel, s)))
            .collect::<Vec<_>>()
    })
    .await
    .context("scanning vault")?;

    let mut conn = db.acquire().await?;
    let known: HashMap<String, (i64, i64)> = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT path, file_size, file_mtime FROM notes WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|(p, s, m)| (p, (s, m)))
    .collect();
    let mut seen = std::collections::HashSet::new();
    let mut changed = 0;
    for (rel, (size, mtime)) in &files {
        seen.insert(rel.clone());
        if known.get(rel) == Some(&(*size, *mtime)) {
            continue;
        }
        if index_file(&mut conn, &root, user_id, rel).await.is_ok() {
            changed += 1;
        }
    }
    for path in known.keys().filter(|p| !seen.contains(*p)) {
        let id: Option<String> = sqlx::query_scalar("SELECT id FROM notes WHERE user_id = ? AND path = ?")
            .bind(user_id)
            .bind(path)
            .fetch_optional(&mut *conn)
            .await?;
        if let Some(id) = id {
            unindex(&mut conn, &id).await?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Scans every person's vault.
pub async fn scan_all(db: &SqlitePool, data_dir: &Path) -> AppResult<usize> {
    let users: Vec<String> = sqlx::query_scalar("SELECT id FROM users").fetch_all(db).await?;
    let mut n = 0;
    for u in users {
        n += scan(db, data_dir, &u).await?;
    }
    Ok(n)
}

pub async fn unindex(conn: &mut SqliteConnection, id: &str) -> AppResult<()> {
    sqlx::query("DELETE FROM notes_fts WHERE note_id = ?").bind(id).execute(&mut *conn).await?;
    sqlx::query("DELETE FROM notes WHERE id = ?").bind(id).execute(&mut *conn).await?;
    Ok(())
}

/// Reads one file and writes its index rows. A file without an `id` in its
/// front matter (or with one another file already uses) gets a new id
/// written back, so ids stay stable across renames. Returns the note's id.
pub async fn index_file(
    conn: &mut SqliteConnection,
    root: &Path,
    user_id: &str,
    rel: &str,
) -> AppResult<String> {
    index_file_chosen(conn, root, user_id, rel, &[]).await
}

/// [`index_file`], when the person just picked the note's projects by id in
/// the app: those win when a title in the front matter matches more than one.
pub async fn index_file_chosen(
    conn: &mut SqliteConnection,
    root: &Path,
    user_id: &str,
    rel: &str,
    chosen: &[String],
) -> AppResult<String> {
    let text = read_file(root, rel).await?;
    let (mut fm, body) = markdown::split(&text);

    // An id is taken when another person's note has it, or another file in
    // this vault still does (a copied file). A row whose file is gone is this
    // file after a move or rename, so the id stays.
    let taken = match &fm.id {
        Some(id) => {
            match sqlx::query_as::<_, (String, String)>("SELECT user_id, path FROM notes WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
            {
                Some((owner, path)) => owner != user_id || (path != rel && root.join(&path).exists()),
                None => false,
            }
        }
        None => true,
    };
    if taken {
        fm.id = Some(time::new_id());
        write_file(root, rel, &markdown::render(&fm, &body)).await?;
    }
    let id = fm.id.clone().unwrap();
    let (size, mtime) = stat(&root.join(rel)).ok_or(AppError::NotFound)?;
    write_index(conn, user_id, &id, rel, &fm, &body, size, mtime, chosen).await?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
async fn write_index(
    conn: &mut SqliteConnection,
    user_id: &str,
    id: &str,
    rel: &str,
    fm: &FrontMatter,
    body: &str,
    size: i64,
    mtime: i64,
    chosen: &[String],
) -> AppResult<()> {
    let title = Path::new(rel).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let folder = rel.split('/').next().filter(|_| rel.contains('/')).unwrap_or("").to_string();
    let file_time = mtime_rfc3339(mtime);
    let created = fm.created.clone().unwrap_or_else(|| file_time.clone());
    let updated = fm.updated.clone().filter(|u| *u >= file_time).unwrap_or(file_time);
    let mut tags: Vec<String> =
        fm.tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    tags.dedup();

    // Drop a stale row for this path under another id (a file whose id changed).
    sqlx::query("DELETE FROM notes WHERE user_id = ? AND path = ? AND id <> ?")
        .bind(user_id)
        .bind(rel)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "INSERT INTO notes (id, user_id, path, title, folder, tags, excerpt, file_size, file_mtime, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET path = excluded.path, title = excluded.title, folder = excluded.folder,
             tags = excluded.tags, excerpt = excluded.excerpt, file_size = excluded.file_size,
             file_mtime = excluded.file_mtime, created_at = excluded.created_at, updated_at = excluded.updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(rel)
    .bind(&title)
    .bind(&folder)
    .bind(serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into()))
    .bind(markdown::excerpt(body))
    .bind(size)
    .bind(mtime)
    .bind(&created)
    .bind(&updated)
    .execute(&mut *conn)
    .await?;

    // Filing a note under a project shares it with everyone on that project,
    // so a title must never pick up a stranger's project of the same name
    // (anyone here can make one and share it with you).
    let before: Vec<String> = sqlx::query_scalar("SELECT project_id FROM note_projects WHERE note_id = ?")
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM note_projects WHERE note_id = ?").bind(id).execute(&mut *conn).await?;
    for p in &fm.projects {
        let candidates: Vec<(String, String)> = sqlx::query_as(
            "SELECT p.id, p.owner_id FROM projects p JOIN project_access a ON a.project_id = p.id
             WHERE a.user_id = ? AND p.title = ? COLLATE NOCASE ORDER BY p.created_at",
        )
        .bind(user_id)
        .bind(p.trim())
        .fetch_all(&mut *conn)
        .await?;
        for project_id in file_under(&candidates, user_id, chosen, &before) {
            sqlx::query("INSERT OR IGNORE INTO note_projects (note_id, project_id) VALUES (?, ?)")
                .bind(id)
                .bind(project_id)
                .execute(&mut *conn)
                .await?;
        }
    }

    sqlx::query("DELETE FROM note_links WHERE note_id = ?").bind(id).execute(&mut *conn).await?;
    for link in markdown::wiki_links(body) {
        sqlx::query("INSERT INTO note_links (note_id, target_kind, target_title) VALUES (?, ?, ?)")
            .bind(id)
            .bind(if link.project { "project" } else { "note" })
            .bind(&link.title)
            .execute(&mut *conn)
            .await?;
    }

    sqlx::query("DELETE FROM notes_fts WHERE note_id = ?").bind(id).execute(&mut *conn).await?;
    sqlx::query("INSERT INTO notes_fts (note_id, title, body) VALUES (?, ?, ?)")
        .bind(id)
        .bind(&title)
        .bind(body)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Which of the projects called by one front-matter title a note is filed
/// under. `candidates` are (id, owner) pairs the note's owner can see. In
/// order: the ones just picked in the app, the owner's own, the ones it was
/// already filed under, or a shared one when it is the only one by that name.
/// Two shared projects with the same name and nothing to choose between them
/// file the note under neither.
fn file_under(
    candidates: &[(String, String)],
    owner: &str,
    chosen: &[String],
    before: &[String],
) -> Vec<String> {
    let pick = |keep: &dyn Fn(&(String, String)) -> bool| -> Vec<String> {
        candidates.iter().filter(|c| keep(c)).map(|(id, _)| id.clone()).collect()
    };
    for found in [
        pick(&|(id, _)| chosen.contains(id)),
        pick(&|(_, o)| o == owner),
        pick(&|(id, _)| before.contains(id)),
    ] {
        if !found.is_empty() {
            return found;
        }
    }
    if candidates.len() == 1 { vec![candidates[0].0.clone()] } else { vec![] }
}

/// Replaces `[[Old]]`, `[[Old|…]]` and `[[Old#…]]` with the new title.
/// `prefix` is `""` for notes and `"@"` for projects.
pub fn rename_links(body: &str, prefix: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    let needle = format!("[[{prefix}");
    while let Some(i) = rest.find(&needle) {
        let after = &rest[i + needle.len()..];
        let matches = after.len() >= old.len()
            && after.is_char_boundary(old.len())
            && after[..old.len()].eq_ignore_ascii_case(old)
            && matches!(after[old.len()..].chars().next(), Some(']' | '|' | '#'));
        out.push_str(&rest[..i + needle.len()]);
        if matches {
            out.push_str(new);
            rest = &after[old.len()..];
        } else {
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_paths() {
        assert_eq!(clean_rel("Knowledge/Cars/Brakes.md").as_deref(), Some("Knowledge/Cars/Brakes.md"));
        assert_eq!(clean_rel("a\\b.md").as_deref(), Some("a/b.md"));
        assert_eq!(clean_rel("../etc/passwd"), None);
        assert_eq!(clean_rel("/abs.md"), None);
        assert_eq!(clean_rel(".obsidian/app.json"), None);
        assert_eq!(clean_folder(" Knowledge / ../Cars: old "), "Knowledge/Cars old");
    }

    #[test]
    fn renames_links() {
        let body = "[[Brakes]] [[brakes|alias]] [[Brakes#Front]] [[Brakes extra]] [[@Brakes]]";
        assert_eq!(
            rename_links(body, "", "Brakes", "Bleeding"),
            "[[Bleeding]] [[Bleeding|alias]] [[Bleeding#Front]] [[Brakes extra]] [[@Brakes]]"
        );
        assert_eq!(rename_links("[[@Car]] [[Car]]", "@", "Car", "Beetle"), "[[@Beetle]] [[Car]]");
    }
}
