//! Markdown written for a model to read: compact, with ids where a follow-up
//! call would need one, and nothing the model can't act on.

use std::fmt::Write;

use crate::{
    board::{self, Status},
    dashboard::Dashboard,
    notes::{NoteDetail, NoteSummary, ProjectNotes},
    projects::{Project, ProjectDetail},
    tasks::{ActivityItem, Task, TaskDetail},
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
    let mut line = format!("- **{}** ({}) · id `{}`", p.title, p.status, p.id);
    if p.role != "owner" {
        let _ = write!(line, " · shared by {} ({})", p.owner_name, p.role);
    }
    let _ = write!(line, " · {} open", s.open_tasks);
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
    if let Some(a) = &t.assignee_name {
        bits.push(format!("assigned to {a}"));
    }
    if t.comment_count > 0 {
        bits.push(format!("{} comment{}", t.comment_count, if t.comment_count == 1 { "" } else { "s" }));
    }
    if t.note_count > 0 {
        bits.push(format!("{} linked note{}", t.note_count, if t.note_count == 1 { "" } else { "s" }));
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
        "task.commented" => format!("commented on {}", quoted(p)),
        "task.note_linked" => format!("linked a note to {}", quoted(p)),
        "status.deleted" => format!("removed the {} column", s("name")),
        "status.created" => format!("added the {} column", s("name")),
        "status.renamed" => format!("renamed the {} column to {}", s("from"), s("name")),
        "status.moved" => match p.get("after").and_then(|v| v.as_str()) {
            Some(after) => format!("moved the {} column after {after}", s("name")),
            None => format!("moved the {} column to the start", s("name")),
        },
        "status.recategorised" => {
            format!("made the {} column count as {}", s("name"), category_label(&s("to")))
        }
        "project.created" => "started the project".into(),
        "project.archived" => "archived the project".into(),
        "project.restored" => "brought the project back".into(),
        "project.status_changed" => format!("marked the project {}", s("to")),
        "project.updated" => "edited the project".into(),
        "link.created" => format!("drew a {} link", s("kind")),
        "link.deleted" => format!("removed a {} link", s("kind")),
        "session.planned" => format!("planned a session for {}", s("when")),
        "session.joined" => format!("is joining the session on {}", s("when")),
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
    if p.role != "owner" {
        facts.push(format!(
            "shared with you by {}, as {}",
            p.owner_name,
            if p.role == "editor" { "an editor" } else { "a viewer, so you can't change it" }
        ));
    }
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
    if let Some(warning) = board::done_warning(b.statuses) {
        let _ = writeln!(out, "Note: {warning}\n");
    }

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
    if !d.tasks.is_empty() {
        let _ = write!(out, "\n\nLinked tasks:\n");
        for t in &d.tasks {
            let _ = writeln!(out, "{} · {} · {}", task_line(t), t.project_title, category_label(&t.category));
        }
    }
    out.push('\n');
    out
}

/// One task in full, for `get_task`: details, checklist, the comment log,
/// linked notes and recent history.
pub fn task(d: &TaskDetail, column: &str) -> String {
    let t = &d.task;
    let mut out = format!("# T-{} {}\n\n{} · {column}", t.number, t.title, t.project_title);
    if let Some(p) = &t.priority {
        let _ = write!(out, " · {p}");
    }
    if let Some(due) = &t.due_date {
        let _ = write!(out, " · {}due {due}", if t.overdue { "**overdue**, " } else { "" });
    }
    if let Some(a) = &t.assignee_name {
        let _ = write!(out, " · assigned to {a}");
    }
    if !t.labels.is_empty() {
        let _ = write!(out, " · {}", t.labels.iter().map(|l| l.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    let _ = write!(out, " · id `{}`", t.id);
    if let Some(desc) = t.description.as_deref().filter(|d| !d.trim().is_empty()) {
        let _ = write!(out, "\n\n{}", desc.trim_end());
    }
    if !d.checklist.is_empty() {
        let _ = write!(out, "\n\n## Checklist\n\n");
        for c in &d.checklist {
            let _ = writeln!(out, "- [{}] {}", if c.done { "x" } else { " " }, c.text);
        }
    }
    let _ = write!(out, "\n\n## Log ({})\n\n", d.comments.len());
    if d.comments.is_empty() {
        out.push_str("No comments yet.\n");
    }
    for c in &d.comments {
        let via = if c.via == "ai" { " (via AI)" } else { "" };
        let _ = writeln!(
            out,
            "### {} · {}{via}\n\n{}\n",
            day(&c.created_at),
            c.user_name.as_deref().unwrap_or("Someone"),
            c.body.trim_end()
        );
        for a in &c.attachments {
            let _ = writeln!(out, "- {}", attachment_line(a));
        }
        if !c.attachments.is_empty() {
            out.push('\n');
        }
    }
    if !d.attachments.is_empty() {
        let _ = write!(out, "\n## Photos and files ({})\n\n", d.attachments.len());
        for a in &d.attachments {
            let _ = writeln!(out, "- {}", attachment_line(a));
        }
    }
    if !d.notes.is_empty() || d.hidden_notes > 0 {
        let _ = write!(out, "\n## Linked notes\n\n");
        for n in &d.notes {
            out.push_str(&note_line(n));
            out.push('\n');
        }
        if d.hidden_notes > 0 {
            let _ = writeln!(out, "- and {} not shared with you", d.hidden_notes);
        }
        out.push_str("\nRead one with get_note.\n");
    }
    let history: Vec<&ActivityItem> =
        d.activity.iter().filter(|a| a.kind != "task.commented").take(10).collect();
    if !history.is_empty() {
        let _ = write!(out, "\n## History\n\n");
        for a in history {
            out.push_str(&activity_line(a));
            out.push('\n');
        }
    }
    out
}

/// A stretch of the calendar, one line per occurrence, then hours per project.
pub fn calendar(list: &[crate::calendar::Occurrence], range: &str) -> String {
    use crate::calendar::{hours, line};
    let mut out = format!("# Calendar: {range}\n\n");
    if list.is_empty() {
        out.push_str("Nothing planned.\n");
        return out;
    }
    for o in list {
        out.push_str(&line(o));
        out.push('\n');
    }
    let mut per: Vec<(String, i64)> = Vec::new();
    let mut busy = 0;
    for o in list {
        match &o.event.project_title {
            Some(t) if o.event.kind == "session" => match per.iter_mut().find(|(n, _)| n == t) {
                Some((_, m)) => *m += o.event.minutes,
                None => per.push((t.clone(), o.event.minutes)),
            },
            _ => busy += o.event.minutes,
        }
    }
    per.sort_by_key(|p| std::cmp::Reverse(p.1));
    out.push_str("\n## Planned time per project\n\n");
    for (title, m) in &per {
        out.push_str(&format!("- {title}: {}\n", hours(*m)));
    }
    if busy > 0 {
        out.push_str(&format!("- Busy (only the user sees this): {}\n", hours(busy)));
    }
    out
}

/// One photo or file, on a line: what it is, where it hangs and its id.
fn attachment_line(a: &crate::attachments::Attachment) -> String {
    let kind = if a.is_image { "photo" } else { "file" };
    let mut line = format!("{kind} **{}**", a.name);
    if let Some(c) = a.caption.as_deref() {
        line.push_str(&format!(" “{c}”"));
    }
    line.push_str(&format!(" · {}", size(a.size)));
    if let (Some(w), Some(h)) = (a.width, a.height) {
        line.push_str(&format!(" · {w}×{h}"));
    }
    match (&a.task_number, &a.task_title, &a.note_title) {
        (Some(n), Some(t), _) => line.push_str(&format!(" · on T-{n} {t}")),
        (_, _, Some(n)) => line.push_str(&format!(" · in note “{n}”")),
        _ if a.row_id.is_some() => line.push_str(" · on a database row"),
        _ => {}
    }
    if a.comment_id.is_some() {
        line.push_str(" (with a comment)");
    }
    let when = a.taken_at.as_deref().map(|t| format!("taken {}", &t[..10.min(t.len())]));
    let when = when.unwrap_or_else(|| format!("added {}", day(&a.created_at)));
    line.push_str(&format!(" · {when}"));
    if let Some(by) = &a.added_by_name {
        line.push_str(&format!(" by {by}"));
    }
    line.push_str(&format!(" · id `{}`", a.id));
    line
}

fn size(bytes: i64) -> String {
    match bytes {
        b if b >= 1024 * 1024 => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
        b if b >= 1024 => format!("{} KB", b / 1024),
        b => format!("{b} bytes"),
    }
}

/// A list of photos and files.
pub fn attachments(list: &[crate::attachments::Attachment]) -> String {
    if list.is_empty() {
        return "No photos or files.".into();
    }
    let mut out = String::new();
    for a in list {
        let _ = writeln!(out, "- {}", attachment_line(a));
    }
    out
}

/// Search results, kind by kind, with ids for follow-up calls.
pub fn search(q: &str, hits: &[crate::search::SearchHit]) -> String {
    let mut out = format!("Results for “{q}”:\n");
    let groups: [(&str, &[&str]); 5] = [
        ("Projects", &["project"]),
        ("Tasks", &["task"]),
        ("Comments in task logs", &["comment"]),
        ("Notes", &["note"]),
        ("Database rows", &["row"]),
    ];
    for (title, kinds) in groups {
        let list: Vec<_> = hits.iter().filter(|h| kinds.contains(&h.kind.as_str())).collect();
        let _ = write!(out, "\n## {title} ({})\n\n", list.len());
        for h in list {
            let project = h.project_title.as_deref().unwrap_or("");
            let line = match h.kind.as_str() {
                "project" => {
                    let mut l =
                        format!("- **{}** ({}) · id `{}`", h.title, h.context.as_deref().unwrap_or(""), h.id);
                    if let Some(s) = &h.snippet {
                        let _ = write!(l, " · {s}");
                    }
                    l
                }
                "task" => format!(
                    "- T-{} {} · {project} · {} · id `{}`",
                    h.task_number.unwrap_or(0),
                    h.title,
                    h.context.as_deref().unwrap_or(""),
                    h.id
                ),
                "comment" => format!(
                    "- On T-{} {} · {project} · {} · task id `{}`\n  {}",
                    h.task_number.unwrap_or(0),
                    h.title,
                    day(&h.updated_at),
                    h.task_id.as_deref().unwrap_or(""),
                    h.snippet.as_deref().unwrap_or("")
                ),
                "note" => {
                    let mut l =
                        format!("- **{}** · id `{}` · {}", h.title, h.id, h.context.as_deref().unwrap_or(""));
                    if let Some(s) = h.snippet.as_deref().filter(|s| !s.is_empty()) {
                        let _ = write!(l, "\n  {s}");
                    }
                    l
                }
                _ => format!(
                    "- **{}** · {project} · database {} · row id `{}`",
                    h.title,
                    h.context.as_deref().unwrap_or(""),
                    h.id
                ),
            };
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// The Today screen as markdown.
pub fn today(t: &crate::today::Today) -> String {
    let mut out = format!("# Today, {}\n", t.date);
    let tasks = |out: &mut String, title: &str, list: &[Task]| {
        if list.is_empty() {
            return;
        }
        let _ = write!(out, "\n## {title} ({})\n\n", list.len());
        for task in list.iter().take(40) {
            let _ = writeln!(out, "{} · {} · id `{}`", task_line(task), task.project_title, task.id);
        }
    };
    tasks(&mut out, "In progress", &t.now);
    tasks(&mut out, "Blocked", &t.blocked);
    tasks(&mut out, &format!("Late or due in the next {} days", crate::today::DUE_DAYS), &t.due);
    if !t.sessions.is_empty() {
        let _ = write!(out, "\n## Today's sessions ({})\n\n", t.sessions.len());
        for o in &t.sessions {
            out.push_str(&crate::calendar::line(o));
            out.push('\n');
        }
    }
    if !t.quiet.is_empty() {
        let _ = write!(
            out,
            "\n## Gone quiet: active, nothing for {}+ days ({})\n\n",
            crate::today::QUIET_DAYS,
            t.quiet.len()
        );
        for p in &t.quiet {
            let tasks = if p.total_tasks == 0 {
                "no tasks yet".to_string()
            } else {
                format!("{} open", p.open_tasks)
            };
            let _ =
                writeln!(out, "- **{}** · quiet {} days · {tasks} · id `{}`", p.title, p.quiet_days, p.id);
        }
    }
    if !t.inbox.is_empty() {
        let _ = write!(out, "\n## Inbox, not filed yet ({})\n\n", t.inbox.len());
        for n in &t.inbox {
            out.push_str(&note_line(n));
            out.push('\n');
        }
    }
    if t.now.is_empty()
        && t.blocked.is_empty()
        && t.due.is_empty()
        && t.sessions.is_empty()
        && t.quiet.is_empty()
        && t.inbox.is_empty()
    {
        out.push_str("\nNothing in progress, blocked, due or waiting in the Inbox.\n");
    }
    out
}
