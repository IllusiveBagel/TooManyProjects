//! MCP access to whiteboards: read one as a list of what's on it, and draw
//! on one with shapes, sticky notes, text and arrows that join them. Places
//! things in rows below what's there when no position is given.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::SqliteConnection;

use super::{bad, find_project, find_task};
use crate::{
    auth::User,
    error::AppResult,
    whiteboards::{self, ElementChange, Whiteboard},
};

/// One thing to add to a board, or the changes to make to one.
#[derive(Deserialize, JsonSchema, Default, Clone)]
pub struct DrawElement {
    /// To add: an id you choose (letters, digits, - or _) so arrows in the same call can
    /// point at it. To update: the element's id from get_whiteboard, or its words.
    pub id: Option<String>,
    /// `box`, `ellipse`, `diamond`, `sticky` (a sticky note), `text` or `arrow`. Needed to add.
    pub kind: Option<String>,
    /// The words on a shape or sticky note, or the text itself. Use \n for a new line.
    pub text: Option<String>,
    /// Top-left corner in board units. Leave out to place it below what's there.
    pub x: Option<f64>,
    pub y: Option<f64>,
    /// Size. Shapes default to 180 × 76, sticky notes 220 × 140.
    pub w: Option<f64>,
    pub h: Option<f64>,
    /// `ink` or a theme colour `slot-1` to `slot-8`.
    pub color: Option<String>,
    /// Shapes: `none`, `tint` (default) or `solid`.
    pub fill: Option<String>,
    /// Text: `s`, `m` (default), `l` or `xl`.
    pub size: Option<String>,
    /// Text: true for the handwritten look.
    pub hand: Option<bool>,
    /// Arrows: the element each end joins (an id, or a shape's words).
    pub from: Option<String>,
    pub to: Option<String>,
    /// Arrows: a short label in the middle.
    pub label: Option<String>,
    pub dashed: Option<bool>,
    /// Arrows: false for a plain line.
    pub head: Option<bool>,
    /// Shapes: the task it stands for, as `T-12` (in the board's project) or a task id.
    /// "" removes the link.
    pub task: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct DrawArgs {
    /// The whiteboard: its id, or its name (any case).
    pub board: String,
    /// The project it's in (id or title). Needed to make a new board, or when two boards share a name.
    pub project: Option<String>,
    /// Make the board in `project` if none has that name.
    #[serde(default)]
    pub create: bool,
    /// Things to add, in order.
    #[serde(default)]
    pub add: Vec<DrawElement>,
    /// Changes to things already there: give `id` and only the fields to change.
    #[serde(default)]
    pub update: Vec<DrawElement>,
    /// Things to remove: ids, or their words.
    #[serde(default)]
    pub delete: Vec<String>,
}

const SLOTS: &[&str] =
    &["ink", "slot-1", "slot-2", "slot-3", "slot-4", "slot-5", "slot-6", "slot-7", "slot-8"];
const GAP: f64 = 60.0;
const PER_ROW: usize = 4;

/// A board by id, or by name (any case) among those you can see, in `project` when given.
pub(super) async fn find_board(
    conn: &mut SqliteConnection,
    user: &User,
    key: &str,
    project: Option<&str>,
) -> AppResult<Option<Whiteboard>> {
    let key = key.trim();
    let all = whiteboards::all_visible(conn, &user.id).await?;
    if let Some(b) = all.iter().find(|b| b.id == key) {
        return Ok(Some(b.clone()));
    }
    let project_id = match project {
        Some(p) => Some(find_project(conn, user, p).await?.id),
        None => None,
    };
    let hits: Vec<&Whiteboard> = all
        .iter()
        .filter(|b| {
            b.title.eq_ignore_ascii_case(key) && project_id.as_ref().is_none_or(|p| &b.project_id == p)
        })
        .collect();
    match hits.as_slice() {
        [one] => Ok(Some((*one).clone())),
        [] => Ok(None),
        many => Err(bad(
            "ambiguous_whiteboard",
            format!(
                "More than one whiteboard is called {key}, in {}. Say which project.",
                many.iter().map(|b| b.project_title.as_str()).collect::<Vec<_>>().join(", ")
            ),
        )),
    }
}

pub(super) async fn need_board(
    conn: &mut SqliteConnection,
    user: &User,
    key: &str,
    project: Option<&str>,
) -> AppResult<Whiteboard> {
    if let Some(b) = find_board(conn, user, key, project).await? {
        return Ok(b);
    }
    let names: Vec<String> = whiteboards::all_visible(conn, &user.id)
        .await?
        .into_iter()
        .map(|b| format!("{} ({})", b.title, b.project_title))
        .take(30)
        .collect();
    Err(bad(
        "unknown_whiteboard",
        if names.is_empty() {
            format!(
                "No whiteboard called {key}, and there are none yet. draw_on_whiteboard with create: true makes one."
            )
        } else {
            format!("No whiteboard called {key}. There are: {}.", names.join(", "))
        },
    ))
}

fn num(v: &Value, k: &str) -> f64 {
    v.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}
fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str).filter(|t| !t.is_empty())
}
fn words(t: &str) -> String {
    t.replace('\n', " / ")
}

/// The board as text: what's on it, grouped, with ids, places and links.
pub(super) async fn describe(conn: &mut SqliteConnection, board: &Whiteboard) -> AppResult<String> {
    let els = whiteboards::elements(conn, &board.id, None).await?;
    let by_id: HashMap<&str, &whiteboards::Element> = els.iter().map(|e| (e.id.as_str(), e)).collect();
    let name = |id: Option<&str>| -> String {
        match id.and_then(|i| by_id.get(i)) {
            Some(e) => match s(&e.data, "text") {
                Some(t) => format!("“{}” [{}]", words(t), e.id),
                None => format!("{} [{}]", e.kind, e.id),
            },
            None => "a loose end".into(),
        }
    };
    let mut out = format!(
        "# {} ({})\nWhiteboard id {}. {} things on it. Last changed {} by {}{}.\n",
        board.title,
        board.project_title,
        board.id,
        els.len(),
        board.updated_at,
        board.updated_by_name.as_deref().unwrap_or("someone"),
        if board.updated_via == "ai" { " via AI" } else { "" },
    );
    if els.is_empty() {
        out.push_str("\nIt's empty.\n");
        return Ok(out);
    }
    let mut shapes = vec![];
    let mut stickies = vec![];
    let mut texts = vec![];
    let mut arrows = vec![];
    let (mut pens, mut photos) = (0, 0);
    for e in &els {
        let d = &e.data;
        let color = s(d, "color").unwrap_or("ink");
        let at = format!("at ({:.0}, {:.0})", num(d, "x"), num(d, "y"));
        match e.kind.as_str() {
            "box" | "ellipse" | "diamond" => shapes.push(format!(
                "- [{}] {} “{}” {at}, {:.0}×{:.0}, {color}{}{}",
                e.id,
                e.kind,
                words(s(d, "text").unwrap_or("")),
                num(d, "w"),
                num(d, "h"),
                if s(d, "fill") == Some("solid") { " solid" } else { "" },
                s(d, "taskRef").map(|t| format!(", stands for task {t}")).unwrap_or_default(),
            )),
            "sticky" => {
                stickies.push(format!("- [{}] “{}” {at}, {color}", e.id, words(s(d, "text").unwrap_or(""))))
            }
            "text" => texts.push(format!(
                "- [{}] “{}” {at}, size {}{}",
                e.id,
                words(s(d, "text").unwrap_or("")),
                s(d, "size").unwrap_or("m"),
                if d.get("hand").and_then(Value::as_bool) == Some(true) { ", handwritten" } else { "" }
            )),
            "arrow" => arrows.push(format!(
                "- [{}] {} {} {}{}{}",
                e.id,
                name(s(d, "from")),
                if d.get("head").and_then(Value::as_bool) == Some(false) { "—" } else { "→" },
                name(s(d, "to")),
                s(d, "label").map(|l| format!(", labelled “{l}”")).unwrap_or_default(),
                if d.get("dashed").and_then(Value::as_bool) == Some(true) { ", dashed" } else { "" },
            )),
            "pen" => pens += 1,
            "image" => photos += 1,
            _ => {}
        }
    }
    for (title, list) in [("Shapes", shapes), ("Arrows", arrows), ("Sticky notes", stickies), ("Text", texts)]
    {
        if !list.is_empty() {
            out.push_str(&format!("\n## {title}\n{}\n", list.join("\n")));
        }
    }
    if pens + photos > 0 {
        out.push_str(&format!(
            "\nAlso {pens} pen drawing{} and {photos} photo{} (not described).\n",
            if pens == 1 { "" } else { "s" },
            if photos == 1 { "" } else { "s" }
        ));
    }
    Ok(out)
}

fn new_id() -> String {
    crate::time::new_id().replace('-', "").chars().rev().take(14).collect()
}

struct Live {
    kind: String,
    version: i64,
    data: Value,
}

fn rect(d: &Value) -> (f64, f64, f64, f64) {
    (num(d, "x"), num(d, "y"), num(d, "w"), num(d, "h"))
}

fn check_color(c: &str) -> AppResult<String> {
    if SLOTS.contains(&c) {
        Ok(c.to_string())
    } else {
        Err(bad("invalid_color", format!("Colours are {}.", SLOTS.join(", "))))
    }
}

/// Finds an element by id, or by its words (first line, any case).
fn resolve(live: &HashMap<String, Live>, order: &[String], key: &str) -> AppResult<String> {
    let k = key.trim();
    if live.contains_key(k) {
        return Ok(k.to_string());
    }
    let lk = k.to_lowercase();
    let hits: Vec<&String> = order
        .iter()
        .filter(|id| {
            live.get(*id).and_then(|e| s(&e.data, "text")).is_some_and(|t| {
                let t = t.to_lowercase();
                t == lk || t.lines().next() == Some(lk.as_str()) || t.replace('\n', " ") == lk
            })
        })
        .collect();
    match hits.as_slice() {
        [one] => Ok((*one).clone()),
        [] => Err(bad(
            "unknown_element",
            format!("Nothing on the board is called “{k}”. Use ids from get_whiteboard."),
        )),
        _ => Err(bad("ambiguous_element", format!("More than one thing says “{k}”. Use its id."))),
    }
}

/// Draws on a board as `user`. Returns what changed, in words.
pub(super) async fn draw(conn: &mut SqliteConnection, user: &User, args: &DrawArgs) -> AppResult<String> {
    let board = match find_board(conn, user, &args.board, args.project.as_deref()).await? {
        Some(b) => b,
        None if args.create => {
            let Some(p) = args.project.as_deref() else {
                return Err(bad("need_project", "Say which project to make the whiteboard in."));
            };
            let project = find_project(conn, user, p).await?;
            let title = args.board.trim();
            if title.is_empty() || title.chars().count() > 80 {
                return Err(bad("invalid_title", "Whiteboard names are 1 to 80 characters."));
            }
            whiteboards::create(conn, &user.id, &project.id, title).await?
        }
        None => need_board(conn, user, &args.board, args.project.as_deref()).await?,
    };
    let (board, project) = whiteboards::editable_board(conn, &user.id, &board.id).await?;

    let existing = whiteboards::elements(conn, &board.id, None).await?;
    let mut order: Vec<String> = existing.iter().map(|e| e.id.clone()).collect();
    let mut live: HashMap<String, Live> = existing
        .into_iter()
        .map(|e| (e.id.clone(), Live { kind: e.kind, version: e.version, data: e.data }))
        .collect();
    let mut z = live.values().map(|e| num(&e.data, "z")).fold(0.0, f64::max) + 1.0;
    let mut changed: Vec<String> = vec![];
    let mut gone: Vec<String> = vec![];

    // Where new things go when no position is given: rows below everything.
    let (mut min_x, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
    for e in live.values() {
        if matches!(e.kind.as_str(), "arrow" | "pen") {
            continue;
        }
        let (x, y, _, h) = rect(&e.data);
        min_x = min_x.min(x);
        max_y = max_y.max(y + h.max(30.0));
    }
    if !min_x.is_finite() {
        min_x = 0.0;
        max_y = -GAP;
    }
    let mut cursor = (min_x, max_y + GAP * 1.5);
    let mut row_h: f64 = 0.0;
    let mut in_row = 0;

    let task_link = async |conn: &mut SqliteConnection, t: &str| -> AppResult<Option<(String, String)>> {
        if t.trim().is_empty() {
            return Ok(None);
        }
        let id = find_task(conn, user, t, Some(&project.id)).await?;
        let task = crate::tasks::visible_task(conn, &user.id, &id).await?;
        if task.project_id != project.id {
            return Err(bad("invalid_task", "A shape can only stand for a task in the board's own project."));
        }
        Ok(Some((task.id, format!("T-{}", task.number))))
    };

    let mut summary = vec![];
    for a in &args.add {
        let kind = a.kind.as_deref().unwrap_or("box").trim().to_lowercase();
        let kind = if kind == "rectangle" || kind == "rect" { "box".to_string() } else { kind };
        let id = match a.id.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
            Some(i) if live.contains_key(i) => {
                return Err(bad("duplicate_id", format!("Something on the board already has the id {i}.")));
            }
            Some(i) => i.to_string(),
            None => new_id(),
        };
        let color =
            check_color(a.color.as_deref().unwrap_or(if kind == "sticky" { "slot-6" } else { "ink" }))?;
        let mut d = Map::new();
        d.insert("z".into(), json!(z));
        z += 1.0;
        d.insert("color".into(), json!(color));
        match kind.as_str() {
            "box" | "ellipse" | "diamond" | "sticky" | "text" => {
                let (dw, dh) = match kind.as_str() {
                    "sticky" => (220.0, 140.0),
                    "diamond" => (160.0, 110.0),
                    "text" => (0.0, 0.0),
                    _ => (180.0, 76.0),
                };
                let (w, h) = (a.w.unwrap_or(dw).max(16.0), a.h.unwrap_or(dh).max(16.0));
                let text = a.text.clone().unwrap_or_default();
                let (x, y) = match (a.x, a.y) {
                    (Some(x), Some(y)) => (x, y),
                    _ => {
                        if in_row == PER_ROW {
                            cursor = (min_x, cursor.1 + row_h + GAP);
                            row_h = 0.0;
                            in_row = 0;
                        }
                        let at = cursor;
                        let width = if kind == "text" {
                            text.lines().map(str::len).max().unwrap_or(4) as f64 * 11.0
                        } else {
                            w
                        };
                        cursor.0 += width + GAP;
                        row_h = row_h.max(if kind == "text" { 40.0 } else { h });
                        in_row += 1;
                        at
                    }
                };
                d.insert("x".into(), json!(x.round()));
                d.insert("y".into(), json!(y.round()));
                if kind == "text" {
                    if text.trim().is_empty() {
                        return Err(bad("invalid_element", "Text needs some words."));
                    }
                    let size = a.size.as_deref().unwrap_or("m");
                    if !["s", "m", "l", "xl"].contains(&size) {
                        return Err(bad("invalid_size", "Text sizes are s, m, l and xl."));
                    }
                    d.insert("size".into(), json!(size));
                    if a.hand == Some(true) {
                        d.insert("hand".into(), json!(true));
                    }
                } else {
                    d.insert("w".into(), json!(w.round()));
                    d.insert("h".into(), json!(h.round()));
                }
                d.insert("text".into(), json!(text));
                if kind != "sticky" && kind != "text" {
                    let fill = a.fill.as_deref().unwrap_or("tint");
                    if !["none", "tint", "solid"].contains(&fill) {
                        return Err(bad("invalid_fill", "Fills are none, tint and solid."));
                    }
                    d.insert("fill".into(), json!(fill));
                    if let Some(t) = &a.task
                        && let Some((tid, tref)) = task_link(conn, t).await?
                    {
                        d.insert("task".into(), json!(tid));
                        d.insert("taskRef".into(), json!(tref));
                    }
                }
                summary.push(format!("added {kind} “{}” [{id}]", words(&text)));
            }
            "arrow" | "line" => {
                let from = a.from.as_deref().map(|k| resolve(&live, &order, k)).transpose()?;
                let to = a.to.as_deref().map(|k| resolve(&live, &order, k)).transpose()?;
                let (Some(from), Some(to)) = (from, to) else {
                    return Err(bad(
                        "invalid_arrow",
                        "An arrow needs from and to: the ids or words of what it joins.",
                    ));
                };
                let centre = |id: &str| {
                    let (x, y, w, h) = rect(&live[id].data);
                    json!([x + w / 2.0, y + h / 2.0])
                };
                d.insert("from".into(), json!(from));
                d.insert("to".into(), json!(to));
                d.insert("a".into(), centre(&from));
                d.insert("b".into(), centre(&to));
                d.insert("head".into(), json!(kind == "arrow" && a.head != Some(false)));
                if let Some(l) = a.label.as_deref().filter(|l| !l.trim().is_empty()) {
                    d.insert("label".into(), json!(l.trim()));
                }
                if a.dashed == Some(true) {
                    d.insert("dashed".into(), json!(true));
                }
                summary.push(format!("joined [{from}] to [{to}] [{id}]"));
            }
            other => {
                return Err(bad(
                    "invalid_kind",
                    format!(
                        "{other} isn't something I can draw. Use box, ellipse, diamond, sticky, text or arrow."
                    ),
                ));
            }
        }
        let kind = if kind == "line" { "arrow".to_string() } else { kind };
        live.insert(id.clone(), Live { kind, version: 0, data: Value::Object(d) });
        order.push(id.clone());
        changed.push(id);
    }

    for u in &args.update {
        let Some(key) = u.id.as_deref() else {
            return Err(bad("invalid_update", "Each update needs the id (or words) of what to change."));
        };
        let id = resolve(&live, &order, key)?;
        let from = u.from.as_deref().map(|k| resolve(&live, &order, k)).transpose()?;
        let to = u.to.as_deref().map(|k| resolve(&live, &order, k)).transpose()?;
        let link = match &u.task {
            Some(t) => Some(task_link(conn, t).await?),
            None => None,
        };
        let e = live.get_mut(&id).unwrap();
        let d = e.data.as_object_mut().unwrap();
        let mut set = |k: &str, v: Value| {
            d.insert(k.into(), v);
        };
        if let Some(t) = &u.text {
            if e.kind == "arrow" {
                set("label", json!(t));
            } else {
                set("text", json!(t));
            }
        }
        for (k, v) in [("x", u.x), ("y", u.y), ("w", u.w), ("h", u.h)] {
            if let Some(v) = v {
                set(k, json!(v.round()));
            }
        }
        if let Some(c) = &u.color {
            set("color", json!(check_color(c)?));
        }
        if let Some(f) = &u.fill {
            if !["none", "tint", "solid"].contains(&f.as_str()) {
                return Err(bad("invalid_fill", "Fills are none, tint and solid."));
            }
            set("fill", json!(f));
        }
        if let Some(sz) = &u.size {
            if !["s", "m", "l", "xl"].contains(&sz.as_str()) {
                return Err(bad("invalid_size", "Text sizes are s, m, l and xl."));
            }
            set("size", json!(sz));
        }
        if let Some(h) = u.hand {
            set("hand", json!(h));
        }
        if let Some(l) = &u.label {
            set("label", json!(l));
        }
        if let Some(v) = u.dashed {
            set("dashed", json!(v));
        }
        if let Some(v) = u.head {
            set("head", json!(v));
        }
        if let Some(f) = from {
            set("from", json!(f));
        }
        if let Some(t) = to {
            set("to", json!(t));
        }
        match link {
            Some(Some((tid, tref))) => {
                set("task", json!(tid));
                set("taskRef", json!(tref));
            }
            Some(None) => {
                d.remove("task");
                d.remove("taskRef");
            }
            None => {}
        }
        summary.push(format!("changed [{id}]"));
        if !changed.contains(&id) {
            changed.push(id);
        }
    }

    for key in &args.delete {
        let id = resolve(&live, &order, key)?;
        // Arrows joined to it let go, staying where they were.
        for (aid, e) in live.iter_mut() {
            if e.kind != "arrow" || *aid == id {
                continue;
            }
            let d = e.data.as_object_mut().unwrap();
            for end in ["from", "to"] {
                if d.get(end).and_then(Value::as_str) == Some(id.as_str()) {
                    d.remove(end);
                    if !changed.contains(aid) {
                        changed.push(aid.clone());
                    }
                }
            }
        }
        summary.push(format!("removed [{id}]"));
        gone.push(id);
    }

    if changed.is_empty() && gone.is_empty() {
        return Err(bad("nothing_to_draw", "Say what to add, update or delete."));
    }
    let mut changes: Vec<ElementChange> = vec![];
    for id in &changed {
        if gone.contains(id) {
            continue;
        }
        let e = &live[id];
        changes.push(ElementChange {
            id: id.clone(),
            kind: e.kind.clone(),
            version: e.version + 1,
            deleted: false,
            data: e.data.clone(),
        });
    }
    for id in &gone {
        let e = &live[id];
        changes.push(ElementChange {
            id: id.clone(),
            kind: e.kind.clone(),
            version: e.version + 1,
            deleted: true,
            data: Value::Null,
        });
    }
    whiteboards::apply_changes(conn, &board.id, &project.id, &user.id, &changes).await?;
    Ok(format!(
        "On {} ({}): {}. Open it in the project's Whiteboards tab; read it back with get_whiteboard.",
        board.title,
        project.title,
        summary.join("; ")
    ))
}
