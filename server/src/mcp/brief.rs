//! Markdown written for a model to read: compact, with ids where a follow-up
//! call would need one, and nothing the model can't act on.

use std::fmt::Write;

use crate::{
    board::Status,
    dashboard::Dashboard,
    notes::{NoteDetail, NoteSummary, ProjectNotes},
    projects::{Project, ProjectDetail},
    tasks::{ActivityItem, Task},
};

const CATEGORY: &[(&str, &str)] =
    &[("in_progress", "In progress"), ("blocked", "Blocked"), ("todo", "To do"), ("done", "Done")];

pub fn category_label(c: &str) -> &str {
    CATEGORY.iter().find(|(k, _)| *k == c).map(|(_, v)| *v).unwrap_or(c)
}

fn day(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// One line per project, for `list_projects`.
pub fn project_line(p: &Project) -> String {
    let s = &p.stats;
    let mut line = format!("- **{}** ({}) · id `{}` · {} open", p.title, p.status, p.id, s.open_tasks);
    if s.in_progress_tasks > 0 {
        let _ = write!(line, ", {} in progress", s.in_progress_tasks);
    }
    if s.overdue_tasks > 0 {
        let _ = write!(line, ", **{} overdue**", s.overdue_tasks);
    }
    let _ = write!(line, ", {} done", s.done_tasks);
    if let Some(t) = &p.target_date {
        let _ = write!(line, " · target {t}");
    }
    let _ = write!(line, " · last active {}", day(&s.last_active_at));
    if let Some(sum) = p.summary.as_deref().filter(|s| !s.is_empty()) {
        let _ = write!(line, "\n  {sum}");
    }
    line
}

/// `T-12 Rebuild carburettor` with its due date, priority and checklist.
pub fn task_line(t: &Task) -> String {
    let mut line = format!("- T-{} {}", t.number, t.title);
    let mut bits = vec![];
    if let Some(p) = &t.priority {
        bits.push(p.clone());
    }
    if let Some(d) = &t.due_date {
        bits.push(if t.overdue { format!("**overdue, due {d}**") } else { format!("due {d}") });
    }
    if t.checklist_total > 0 {
        bits.push(format!("checklist {}/{}", t.checklist_done, t.checklist_total));
    }
    if !t.labels.is_empty() {
        bits.push(t.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    if !bits.is_empty() {
        let _ = write!(line, " ({})", bits.join("; "));
    }
    let _ = write!(line, " · id `{}`", t.id);
    line
}

fn quoted(p: &serde_json::Value) -> String {
    p.get("title").and_then(|t| t.as_str()).map(|t| format!("“{t}”")).unwrap_or_else(|| "a task".into())
}

/// The activity feed in words, matching the web app.
pub fn activity_line(a: &ActivityItem) -> String {
    let p = &a.payload;
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("?").to_string();
    let what = match a.kind.as_str() {
        "task.created" => format!("added {}", quoted(p)),
        "task.moved" => format!("moved {} from {} to {}", quoted(p), s("from"), s("to")),
        "task.completed" => format!("finished {}", quoted(p)),
        "task.updated" => format!("edited {}", quoted(p)),
        "task.deleted" => format!("deleted {}", quoted(p)),
        "status.deleted" => format!("removed the {} column", s("name")),
        "project.created" => "started the project".into(),
        "project.archived" => "archived the project".into(),
        "project.restored" => "brought the project back".into(),
        "project.status_changed" => format!("marked the project {}", s("to")),
        "project.updated" => "edited the project".into(),
        "link.created" => format!("drew a {} link", s("kind")),
        "link.deleted" => format!("removed a {} link", s("kind")),
        other => other.replace('.', " "),
    };
    let via = if a.via == "ai" { " (via AI)" } else { "" };
    format!("- {} {} {what}{via}", day(&a.created_at), a.user_name.as_deref().unwrap_or("Someone"))
}

pub fn note_line(n: &NoteSummary) -> String {
    let mut line = format!("- **{}** · id `{}` · `{}`", n.title, n.id, n.path);
    if !n.tags.is_empty() {
        let _ = write!(line, " · #{}", n.tags.join(" #"));
    }
    let _ = write!(line, " · updated {}", day(&n.updated_at));
    if let Some(s) = n.snippet.as_deref().filter(|s| !s.is_empty()) {
        let _ = write!(line, "\n  {s}");
    } else if !n.excerpt.is_empty() {
        let _ = write!(line, "\n  {}", n.excerpt);
    }
    line
}

pub struct ProjectBrief<'a> {
    pub detail: &'a ProjectDetail,
    pub statuses: &'a [Status],
    pub tasks: &'a [Task],
    pub dashboard: &'a Dashboard,
    pub notes: &'a ProjectNotes,
    /// `related` and `blocks` links, as (kind, direction, other title).
    pub other_links: &'a [(String, bool, String)],
}

/// Everything about one project an assistant needs to pick up where you left off.
pub fn project(b: &ProjectBrief) -> String {
    let p = &b.detail.project;
    let mut out = String::new();
    let _ = writeln!(out, "# {}\n", p.title);
    let mut facts = vec![format!("Status: {}", p.status), format!("id `{}`", p.id)];
    if let Some(d) = &p.started_at {
        facts.push(format!("started {d}"));
    }
    if let Some(d) = &p.target_date {
        facts.push(format!("target {d}"));
    }
    facts.push(format!("last active {}", day(&p.stats.last_active_at)));
    let _ = writeln!(out, "{}\n", facts.join(" · "));
    if let Some(s) = p.summary.as_deref().filter(|s| !s.is_empty()) {
        let _ = writeln!(out, "{s}\n");
    }
    if let Some(d) = p.description.as_deref().filter(|s| !s.trim().is_empty()) {
        let _ = writeln!(out, "## Description\n\n{}\n", d.trim());
    }

    if !b.detail.parents.is_empty() || !b.detail.children.is_empty() || !b.other_links.is_empty() {
        let _ = writeln!(out, "## Family\n");
        for r in &b.detail.parents {
            let _ = writeln!(out, "- Spawned from **{}** ({}) · id `{}`", r.title, r.status, r.id);
        }
        for r in &b.detail.children {
            let _ = writeln!(out, "- Spawned **{}** ({}) · id `{}`", r.title, r.status, r.id);
        }
        for (kind, outgoing, title) in b.other_links {
            let text = match (kind.as_str(), outgoing) {
                ("blocks", true) => format!("Blocks **{title}**"),
                ("blocks", false) => format!("Blocked by **{title}**"),
                _ => format!("Related to **{title}**"),
            };
            let _ = writeln!(out, "- {text}");
        }
        out.push('\n');
    }

    let open = b.tasks.iter().filter(|t| t.category != "done").count();
    let _ = writeln!(out, "## Board: {open} open, {} done\n", p.stats.done_tasks);
    for (cat, _) in CATEGORY.iter().filter(|(c, _)| *c != "done") {
        for s in b.statuses.iter().filter(|s| s.category == *cat) {
            let mine: Vec<&Task> = b.tasks.iter().filter(|t| t.status_id == s.id).collect();
            if mine.is_empty() {
                continue;
            }
            let _ = writeln!(out, "### {} ({}, {})\n", s.name, category_label(cat), mine.len());
            for t in mine.iter().take(25) {
                let _ = writeln!(out, "{}", task_line(t));
            }
            if mine.len() > 25 {
                let _ = writeln!(out, "- …and {} more", mine.len() - 25);
            }
            out.push('\n');
        }
    }
    let mut done: Vec<&Task> = b.tasks.iter().filter(|t| t.category == "done").collect();
    done.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));
    if !done.is_empty() {
        let _ = writeln!(out, "### Recently finished\n");
        for t in done.iter().take(5) {
            let _ = writeln!(
                out,
                "- T-{} {} (done {})",
                t.number,
                t.title,
                t.completed_at.as_deref().map(day).unwrap_or("?")
            );
        }
        out.push('\n');
    }
    let columns: Vec<String> =
        b.statuses.iter().map(|s| format!("{} ({})", s.name, category_label(&s.category))).collect();
    let _ = writeln!(out, "Columns, in order: {}\n", columns.join(", "));

    let total_notes = b.notes.notes.len() + b.notes.mentions.len();
    if total_notes > 0 {
        let _ = writeln!(out, "## Notes ({total_notes})\n");
        for n in b.notes.notes.iter().take(20) {
            let _ = writeln!(out, "{}", note_line(n));
        }
        if !b.notes.mentions.is_empty() {
            let _ = writeln!(out, "\nMentioned in:\n");
            for n in b.notes.mentions.iter().take(10) {
                let _ = writeln!(out, "{}", note_line(n));
            }
        }
        out.push('\n');
    }

    if !b.dashboard.activity.is_empty() {
        let _ = writeln!(out, "## Recent activity\n");
        for a in b.dashboard.activity.iter().take(12) {
            let _ = writeln!(out, "{}", activity_line(a));
        }
        out.push('\n');
    }
    out.trim_end().to_string() + "\n"
}

/// A shorter view of a relative, for family briefings.
pub fn relative(p: &Project, tasks: &[Task]) -> String {
    let mut out = format!("### {} ({}) · id `{}`\n\n", p.title, p.status, p.id);
    if let Some(s) = p.summary.as_deref().filter(|s| !s.is_empty()) {
        let _ = writeln!(out, "{s}\n");
    }
    let open: Vec<&Task> = tasks.iter().filter(|t| t.category != "done").collect();
    let _ = writeln!(out, "{} open, {} done.", open.len(), p.stats.done_tasks);
    for t in open.iter().take(8) {
        let _ = writeln!(out, "{} [{}]", task_line(t), category_label(&t.category));
    }
    out
}

/// A note with its metadata, for `get_note` and note resources.
pub fn note(d: &NoteDetail) -> String {
    let n = &d.note;
    let mut out = format!("# {}\n\nid `{}` · `{}` · updated {}", n.title, n.id, n.path, day(&n.updated_at));
    if !n.projects.is_empty() {
        let _ = write!(
            out,
            " · projects: {}",
            n.projects.iter().map(|p| p.title.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    if !n.tags.is_empty() {
        let _ = write!(out, " · #{}", n.tags.join(" #"));
    }
    let _ = write!(out, "\n\n---\n\n{}", d.body.trim_end());
    if !d.backlinks.is_empty() {
        let _ = write!(out, "\n\n---\n\nLinked from: ");
        let names: Vec<String> = d.backlinks.iter().map(|b| format!("{} (`{}`)", b.title, b.id)).collect();
        out.push_str(&names.join(", "));
    }
    out.push('\n');
    out
}
