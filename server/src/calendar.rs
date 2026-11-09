//! The weekly calendar: work sessions on projects, busy blocks, and people
//! saying they'll join a session.
//!
//! * A **session** is time on one project. Everyone who can see the project
//!   sees it; its owner and editors plan them; anyone who can see the project
//!   (viewers too) can say they'll join.
//! * A **busy** block is time you can't give to projects. Only you see it.
//!
//! Both can repeat weekly. An event keeps its local wall-clock start, its
//! length and its time zone, and the server expands repeats for whatever range
//! is asked for, so 9 to 5 stays 9 to 5 when the clocks change. One week of a
//! repeat can be taken out (a skip), and joins are per week: the key of an
//! occurrence is the local date it starts on.

use std::collections::{HashMap, HashSet};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqliteConnection};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::{AppError, AppResult},
    projects::{clean_text, editable_project, log_activity, some, via},
    time,
};

/// Longest range one calendar read may cover.
pub const MAX_RANGE_DAYS: i64 = 62;
/// Local start times look like `2026-11-14T13:00`.
pub const LOCAL_FORMAT: &str = "%Y-%m-%dT%H:%M";

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_calendar))
        .routes(routes!(create_event))
        .routes(routes!(get_event, update_event, delete_event))
        .routes(routes!(join_session, leave_session))
}

// ---------------------------------------------------------------------------
// Types

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct CalendarEvent {
    pub id: String,
    /// Who put it in the calendar.
    pub user_id: String,
    pub user_name: String,
    /// `session` (time on a project) or `busy` (only you see these).
    pub kind: String,
    /// Set for sessions.
    pub project_id: Option<String>,
    pub project_title: Option<String>,
    pub project_color: Option<String>,
    /// What the time is for: "Rear brakes", "Work".
    pub title: Option<String>,
    /// First occurrence, local wall-clock time: `2026-11-14T13:00`.
    pub start_local: String,
    pub minutes: i64,
    /// IANA time zone, such as `Europe/London`.
    pub tz: String,
    /// Repeats weekly on these days: bit 0 is Monday, bit 6 Sunday. 0 = once.
    pub repeat_days: i64,
    /// Last day a repeat may start on, local `YYYY-MM-DD`.
    pub repeat_until: Option<String>,
    /// `web` or `ai`.
    pub via: String,
    pub created_at: String,
    pub updated_at: String,
    /// You may change or delete it.
    pub can_edit: bool,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Joiner {
    pub user_id: String,
    pub name: String,
}

/// One time an event happens.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct Occurrence {
    pub event: CalendarEvent,
    /// The local date it starts on, `YYYY-MM-DD`. Joins and skips use it.
    pub occurrence: String,
    /// UTC, RFC 3339.
    pub start_at: String,
    pub end_at: String,
    /// People who said they'll join (sessions only), in the order they did.
    pub joiners: Vec<Joiner>,
    /// You are one of the joiners.
    pub joined: bool,
    /// You can say you'll join: a session someone else planned.
    pub can_join: bool,
}

#[derive(Deserialize, IntoParams)]
pub struct CalendarQuery {
    /// Start of the range, RFC 3339.
    pub from: String,
    /// End of the range, RFC 3339. At most 62 days after `from`.
    pub to: String,
    /// Only this project's sessions.
    pub project_id: Option<String>,
    /// Only what you put in or are joining.
    #[serde(default)]
    pub mine: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateEventRequest {
    /// `session` or `busy`.
    pub kind: String,
    /// Required for a session; not allowed for busy time.
    pub project_id: Option<String>,
    pub title: Option<String>,
    /// Local wall-clock start, `2026-11-14T13:00`.
    pub start_local: String,
    /// 15 to 1440.
    pub minutes: i64,
    /// IANA time zone, such as `Europe/London`.
    pub tz: String,
    /// Weekly repeat days, bit 0 Monday to bit 6 Sunday. Default 0 (once).
    #[serde(default)]
    pub repeat_days: i64,
    /// Last day a repeat may start on, `YYYY-MM-DD`.
    pub repeat_until: Option<String>,
}

/// Fields left out stay as they are; `null` clears `title` or `repeat_until`.
/// Changes to a repeating event apply to every week.
#[derive(Deserialize, ToSchema)]
pub struct UpdateEventRequest {
    /// Move a session to another project you can edit.
    pub project_id: Option<String>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub title: Option<Option<String>>,
    pub start_local: Option<String>,
    pub minutes: Option<i64>,
    pub tz: Option<String>,
    pub repeat_days: Option<i64>,
    #[serde(default, deserialize_with = "some")]
    #[schema(value_type = Option<String>)]
    pub repeat_until: Option<Option<String>>,
}

#[derive(Deserialize, IntoParams)]
pub struct DeleteQuery {
    /// On a repeating event, take out only the week starting on this local
    /// date (`YYYY-MM-DD`). Leave out to delete every week.
    pub occurrence: Option<String>,
}

#[derive(FromRow)]
struct EventRow {
    id: String,
    user_id: String,
    user_name: String,
    kind: String,
    project_id: Option<String>,
    project_title: Option<String>,
    project_color: Option<String>,
    title: Option<String>,
    start_local: String,
    minutes: i64,
    tz: String,
    repeat_days: i64,
    repeat_until: Option<String>,
    via: String,
    created_at: String,
    updated_at: String,
    role: Option<String>,
}

/// `?1` is the viewer.
const EVENT_SELECT: &str = "SELECT e.id, e.user_id, u.name AS user_name, e.kind, e.project_id,
        p.title AS project_title, p.color AS project_color, e.title, e.start_local, e.minutes, e.tz,
        e.repeat_days, e.repeat_until, e.via, e.created_at, e.updated_at, a.role AS role
    FROM calendar_events e
    JOIN users u ON u.id = e.user_id
    LEFT JOIN projects p ON p.id = e.project_id
    LEFT JOIN project_access a ON a.project_id = e.project_id AND a.user_id = ?1";

/// Busy time is its owner's alone; sessions follow their project.
const VISIBLE: &str = "((e.kind = 'busy' AND e.user_id = ?1) OR (e.kind = 'session' AND a.role IS NOT NULL))";

fn finish(row: EventRow, viewer: &str) -> CalendarEvent {
    let mine = row.user_id == viewer;
    let role = row.role.as_deref().unwrap_or("");
    let can_edit = if row.kind == "busy" { mine } else { (mine && role != "viewer") || role == "owner" };
    CalendarEvent {
        can_edit,
        id: row.id,
        user_id: row.user_id,
        user_name: row.user_name,
        kind: row.kind,
        project_id: row.project_id,
        project_title: row.project_title,
        project_color: row.project_color,
        title: row.title,
        start_local: row.start_local,
        minutes: row.minutes,
        tz: row.tz,
        repeat_days: row.repeat_days,
        repeat_until: row.repeat_until,
        via: row.via,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

// ---------------------------------------------------------------------------
// Times and repeats

fn bad(code: &'static str, msg: impl Into<String>) -> AppError {
    AppError::BadRequest(code, msg.into())
}

/// When an event happens, worked out from its stored fields.
#[derive(Clone, Debug)]
pub struct Times {
    pub start: NaiveDateTime,
    pub minutes: i64,
    pub tz: Tz,
    pub repeat_days: u8,
    pub until: Option<NaiveDate>,
}

/// A local time in `tz` as UTC. In the hour skipped when clocks go forward
/// the time moves an hour later; in the hour repeated when they go back, the
/// first one counts.
pub fn to_utc(tz: Tz, local: NaiveDateTime) -> DateTime<Utc> {
    let at = match tz.from_local_datetime(&local) {
        LocalResult::Single(t) => t,
        LocalResult::Ambiguous(first, _) => first,
        LocalResult::None => tz
            .from_local_datetime(&(local + Duration::hours(1)))
            .earliest()
            .unwrap_or_else(|| tz.from_utc_datetime(&local)),
    };
    at.with_timezone(&Utc)
}

pub fn parse_tz(tz: &str) -> AppResult<Tz> {
    tz.parse::<Tz>().map_err(|_| bad("invalid_tz", "Time zones look like Europe/London."))
}

pub fn parse_local(s: &str) -> AppResult<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, LOCAL_FORMAT)
        .map_err(|_| bad("invalid_start", "Start times look like 2026-11-14T13:00."))
}

pub fn parse_date(s: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| bad("invalid_date", "Dates look like 2026-11-14."))
}

impl Times {
    pub fn new(
        start_local: &str,
        minutes: i64,
        tz: &str,
        repeat_days: i64,
        until: Option<&str>,
    ) -> AppResult<Self> {
        let start = parse_local(start_local)?;
        if !(15..=1440).contains(&minutes) {
            return Err(bad("invalid_length", "A block lasts from 15 minutes to 24 hours."));
        }
        let tz = parse_tz(tz)?;
        if !(0..=127).contains(&repeat_days) {
            return Err(bad("invalid_repeat", "Repeat days are a Monday-to-Sunday bitmask, 0 to 127."));
        }
        // An end date only means something on a repeat.
        let until = match (repeat_days, until) {
            (0, _) | (_, None) => None,
            (_, Some(u)) => {
                let u = parse_date(u)?;
                if u < start.date() {
                    return Err(bad("invalid_until", "A repeat can't end before it starts."));
                }
                Some(u)
            }
        };
        Ok(Self { start, minutes, tz, repeat_days: repeat_days as u8, until })
    }

    fn of(e: &CalendarEvent) -> AppResult<Self> {
        Self::new(&e.start_local, e.minutes, &e.tz, e.repeat_days, e.repeat_until.as_deref())
    }

    pub fn repeats(&self) -> bool {
        self.repeat_days != 0
    }

    /// Whether an occurrence starts on this local date.
    pub fn occurs_on(&self, day: NaiveDate) -> bool {
        let first = self.start.date();
        if day < first {
            return false;
        }
        if !self.repeats() {
            return day == first;
        }
        self.repeat_days & (1 << day.weekday().num_days_from_monday()) != 0
            && self.until.is_none_or(|u| day <= u)
    }

    /// The occurrence starting on `day`, in UTC.
    pub fn at(&self, day: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
        let start = to_utc(self.tz, day.and_time(self.start.time()));
        (start, start + Duration::minutes(self.minutes))
    }

    /// Every occurrence overlapping `[from, to)`, as (local date, start, end).
    pub fn between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Vec<(NaiveDate, DateTime<Utc>, DateTime<Utc>)> {
        // A block can last a day, so one starting the day before may reach in.
        let mut day = from.with_timezone(&self.tz).date_naive() - Duration::days(1);
        let last = to.with_timezone(&self.tz).date_naive();
        let mut out = Vec::new();
        while day <= last {
            if self.occurs_on(day) {
                let (s, e) = self.at(day);
                if s < to && e > from {
                    out.push((day, s, e));
                }
            }
            day += Duration::days(1);
        }
        out
    }

    /// Lower bound for range queries.
    fn first_start(&self) -> DateTime<Utc> {
        self.at(self.start.date()).0
    }

    /// Upper bound for range queries; `None` for a repeat with no end.
    fn last_end(&self) -> Option<DateTime<Utc>> {
        match (self.repeats(), self.until) {
            (false, _) => Some(self.at(self.start.date()).1),
            (true, Some(u)) => Some(self.at(u).1),
            (true, None) => None,
        }
    }
}

/// "Sat 14 Nov, 13:00", in the event's own time zone.
pub fn when(tz: Tz, at: DateTime<Utc>) -> String {
    at.with_timezone(&tz).format("%a %-d %b, %H:%M").to_string()
}

/// One line about an occurrence, for the AI: "Sat 14 Nov 13:00–17:00 (4h) · …".
pub fn line(o: &Occurrence) -> String {
    let tz = parse_tz(&o.event.tz).unwrap_or(Tz::UTC);
    let start = DateTime::parse_from_rfc3339(&o.start_at).map(|d| d.with_timezone(&tz));
    let end = DateTime::parse_from_rfc3339(&o.end_at).map(|d| d.with_timezone(&tz));
    let time = match (start, end) {
        (Ok(s), Ok(e)) => format!("{}–{}", s.format("%a %-d %b %H:%M"), e.format("%H:%M")),
        _ => o.start_at.clone(),
    };
    let length = hours(o.event.minutes);
    let repeat = if o.event.repeat_days != 0 {
        format!(", repeats {}", days_text(o.event.repeat_days))
    } else {
        String::new()
    };
    let what = match (o.event.kind.as_str(), &o.event.project_title, &o.event.title) {
        ("session", Some(p), Some(t)) => format!("{p}: {t}"),
        ("session", Some(p), None) => p.clone(),
        (_, _, Some(t)) => format!("Busy: {t}"),
        _ => "Busy".into(),
    };
    let mut people = format!("planned by {}", o.event.user_name);
    if !o.joiners.is_empty() {
        let names: Vec<&str> = o.joiners.iter().map(|j| j.name.as_str()).collect();
        people.push_str(&format!(", joining: {}", names.join(", ")));
    }
    if o.event.kind == "busy" {
        format!("- {time} ({length}{repeat}) · {what}")
    } else {
        format!(
            "- {time} ({length}{repeat}) · {what} · {people} · event `{}` week {}",
            o.event.id, o.occurrence
        )
    }
}

/// "4h", "1h 30m", "45m".
pub fn hours(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// "weekdays", "weekends", "every day" or "Mon, Thu".
pub fn days_text(mask: i64) -> String {
    match mask {
        127 => "every day".into(),
        31 => "weekdays".into(),
        96 => "weekends".into(),
        _ => ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(_, d)| *d)
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Turns day names ("weekdays", "Sat", "mon,wed") into the repeat bitmask.
pub fn parse_days(text: &str) -> AppResult<i64> {
    let mut mask = 0;
    for part in text.split([',', ' ', '/']).map(|p| p.trim().to_lowercase()).filter(|p| !p.is_empty()) {
        mask |= match part.as_str() {
            "weekdays" => 31,
            "weekends" => 96,
            "daily" | "everyday" => 127,
            p => {
                let i = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"]
                    .iter()
                    .position(|d| p.starts_with(d))
                    .ok_or_else(|| bad("invalid_repeat", format!("Unknown day: {p}")))?;
                1 << i
            }
        };
    }
    Ok(mask)
}

fn parse_instant(s: &str, what: &str) -> AppResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&Utc)).map_err(|_| {
        bad("invalid_range", format!("`{what}` is an RFC 3339 time, like 2026-11-09T00:00:00Z."))
    })
}

// ---------------------------------------------------------------------------
// Shared helpers

/// An event the viewer can see, else 404.
pub async fn visible_event(conn: &mut SqliteConnection, viewer: &str, id: &str) -> AppResult<CalendarEvent> {
    let row = sqlx::query_as::<_, EventRow>(&format!("{EVENT_SELECT} WHERE e.id = ?2 AND {VISIBLE}"))
        .bind(viewer)
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(finish(row, viewer))
}

async fn editable_event(conn: &mut SqliteConnection, viewer: &str, id: &str) -> AppResult<CalendarEvent> {
    let event = visible_event(conn, viewer, id).await?;
    if !event.can_edit {
        return Err(AppError::Forbidden(format!(
            "Only {} or the project's owner can change this session.",
            event.user_name
        )));
    }
    Ok(event)
}

async fn skips(conn: &mut SqliteConnection, event_id: &str) -> AppResult<HashSet<String>> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT occurrence FROM calendar_skips WHERE event_id = ?")
        .bind(event_id)
        .fetch_all(conn)
        .await?;
    Ok(rows.into_iter().collect())
}

async fn joins(conn: &mut SqliteConnection, event_id: &str) -> AppResult<HashMap<String, Vec<Joiner>>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT j.occurrence, j.user_id, u.name FROM session_joins j JOIN users u ON u.id = j.user_id
         WHERE j.event_id = ? ORDER BY j.created_at, u.name",
    )
    .bind(event_id)
    .fetch_all(conn)
    .await?;
    let mut out: HashMap<String, Vec<Joiner>> = HashMap::new();
    for (occ, user_id, name) in rows {
        out.entry(occ).or_default().push(Joiner { user_id, name });
    }
    Ok(out)
}

fn occurrence(
    event: &CalendarEvent,
    viewer: &str,
    day: NaiveDate,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    joins: &HashMap<String, Vec<Joiner>>,
) -> Occurrence {
    let key = day.format("%Y-%m-%d").to_string();
    let joiners = joins.get(&key).cloned().unwrap_or_default();
    Occurrence {
        joined: joiners.iter().any(|j| j.user_id == viewer),
        can_join: event.kind == "session" && event.user_id != viewer,
        joiners,
        occurrence: key,
        start_at: time::format(start),
        end_at: time::format(end),
        event: event.clone(),
    }
}

/// Everything in the viewer's calendar between two instants, by start time.
pub async fn occurrences_between(
    conn: &mut SqliteConnection,
    viewer: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    project_id: Option<&str>,
    mine: bool,
) -> AppResult<Vec<Occurrence>> {
    if to <= from {
        return Err(bad("invalid_range", "`to` must come after `from`."));
    }
    if to - from > Duration::days(MAX_RANGE_DAYS) {
        return Err(bad("invalid_range", format!("Ask for at most {MAX_RANGE_DAYS} days at a time.")));
    }
    let project_filter = if project_id.is_some() { " AND e.project_id = ?4" } else { "" };
    let rows = sqlx::query_as::<_, EventRow>(&format!(
        "{EVENT_SELECT} WHERE {VISIBLE} AND e.first_start_at < ?2
            AND (e.last_end_at IS NULL OR e.last_end_at > ?3){project_filter}"
    ))
    .bind(viewer)
    .bind(time::format(to))
    .bind(time::format(from))
    .bind(project_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::new();
    for row in rows {
        let event = finish(row, viewer);
        let times = Times::of(&event)?;
        let skipped = skips(conn, &event.id).await?;
        let joined = joins(conn, &event.id).await?;
        for (day, start, end) in times.between(from, to) {
            let o = occurrence(&event, viewer, day, start, end, &joined);
            if skipped.contains(&o.occurrence) || (mine && event.user_id != viewer && !o.joined) {
                continue;
            }
            out.push(o);
        }
    }
    out.sort_by(|a, b| a.start_at.cmp(&b.start_at).then(a.event.id.cmp(&b.event.id)));
    Ok(out)
}

/// One occurrence of an event, if it happens (and wasn't taken out) that day.
async fn one_occurrence(
    conn: &mut SqliteConnection,
    viewer: &str,
    event: &CalendarEvent,
    day: &str,
) -> AppResult<Occurrence> {
    let date = parse_date(day)?;
    let times = Times::of(event)?;
    if !times.occurs_on(date) || skips(conn, &event.id).await?.contains(day) {
        return Err(bad("no_occurrence", format!("This doesn't happen on {day}.")));
    }
    let (start, end) = times.at(date);
    Ok(occurrence(event, viewer, date, start, end, &joins(conn, &event.id).await?))
}

/// The time zone someone last planned in, for the AI to default to.
pub async fn usual_tz(conn: &mut SqliteConnection, user_id: &str) -> AppResult<String> {
    Ok(sqlx::query_scalar(
        "SELECT tz FROM calendar_events WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await?
    .unwrap_or_else(|| "UTC".into()))
}

fn check_kind(kind: &str, project_id: &Option<String>) -> AppResult<()> {
    match (kind, project_id) {
        ("session", Some(_)) | ("busy", None) => Ok(()),
        ("session", None) => Err(bad("invalid_project", "Pick the project the session is for.")),
        ("busy", Some(_)) => Err(bad("invalid_project", "Busy time isn't tied to a project.")),
        _ => Err(bad("invalid_kind", "Kind is session or busy.")),
    }
}

async fn log_session(
    conn: &mut SqliteConnection,
    event: &CalendarEvent,
    user_id: &str,
    kind: &str,
    at: DateTime<Utc>,
) -> AppResult<()> {
    let Some(project_id) = &event.project_id else { return Ok(()) };
    let tz = parse_tz(&event.tz)?;
    log_activity(
        conn,
        project_id,
        user_id,
        kind,
        serde_json::json!({
            "event_id": event.id,
            "when": when(tz, at),
            "title": event.title,
            "repeats": event.repeat_days != 0,
        }),
    )
    .await
}

// ---------------------------------------------------------------------------
// Handlers

/// Your calendar between two times: sessions on every project you can see,
/// your own busy time, and who's joining. Repeats come back once per week.
#[utoipa::path(get, path = "/calendar", tag = "calendar", params(CalendarQuery),
    responses((status = 200, body = Vec<Occurrence>), (status = 400, body = crate::error::ErrorBody)))]
pub(crate) async fn list_calendar(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<CalendarQuery>,
) -> AppResult<Json<Vec<Occurrence>>> {
    let from = parse_instant(&q.from, "from")?;
    let to = parse_instant(&q.to, "to")?;
    let mut conn = state.db.acquire().await?;
    Ok(Json(occurrences_between(&mut conn, &user.id, from, to, q.project_id.as_deref(), q.mine).await?))
}

/// Plan a session on a project, or block out busy time.
#[utoipa::path(post, path = "/calendar/events", tag = "calendar", request_body = CreateEventRequest,
    responses((status = 201, body = CalendarEvent), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn create_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateEventRequest>,
) -> AppResult<(StatusCode, Json<CalendarEvent>)> {
    check_kind(&req.kind, &req.project_id)?;
    let times =
        Times::new(&req.start_local, req.minutes, &req.tz, req.repeat_days, req.repeat_until.as_deref())?;
    let title = clean_text(req.title, 200, "invalid_title")?;
    let mut tx = state.db.begin().await?;
    if let Some(p) = &req.project_id {
        editable_project(&mut tx, &user.id, p).await?;
    }
    let id = time::new_id();
    let now = time::now();
    sqlx::query(
        "INSERT INTO calendar_events (id, user_id, kind, project_id, title, start_local, minutes, tz, repeat_days,
            repeat_until, first_start_at, last_end_at, via, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.kind)
    .bind(&req.project_id)
    .bind(&title)
    .bind(times.start.format(LOCAL_FORMAT).to_string())
    .bind(times.minutes)
    .bind(times.tz.name())
    .bind(times.repeat_days as i64)
    .bind(times.until.map(|u| u.format("%Y-%m-%d").to_string()))
    .bind(time::format(times.first_start()))
    .bind(times.last_end().map(time::format))
    .bind(via())
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let event = visible_event(&mut tx, &user.id, &id).await?;
    log_session(&mut tx, &event, &user.id, "session.planned", times.first_start()).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(event)))
}

/// One event, as you see it.
#[utoipa::path(get, path = "/calendar/events/{id}", tag = "calendar", params(("id" = String, Path)),
    responses((status = 200, body = CalendarEvent), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn get_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<CalendarEvent>> {
    let mut conn = state.db.acquire().await?;
    Ok(Json(visible_event(&mut conn, &user.id, &id).await?))
}

/// Change an event. On a repeat, every week changes.
#[utoipa::path(patch, path = "/calendar/events/{id}", tag = "calendar", params(("id" = String, Path)),
    request_body = UpdateEventRequest,
    responses((status = 200, body = CalendarEvent), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn update_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateEventRequest>,
) -> AppResult<Json<CalendarEvent>> {
    let mut tx = state.db.begin().await?;
    let current = editable_event(&mut tx, &user.id, &id).await?;
    let project_id = match req.project_id {
        Some(p) if current.kind == "busy" => {
            return Err(bad("invalid_project", format!("Busy time isn't tied to a project ({p}).")));
        }
        Some(p) => {
            if Some(&p) != current.project_id.as_ref() {
                editable_project(&mut tx, &user.id, &p).await?;
            }
            Some(p)
        }
        None => current.project_id.clone(),
    };
    let title = match req.title {
        Some(t) => clean_text(t, 200, "invalid_title")?,
        None => current.title.clone(),
    };
    let until = match req.repeat_until {
        Some(u) => u,
        None => current.repeat_until.clone(),
    };
    let times = Times::new(
        req.start_local.as_deref().unwrap_or(&current.start_local),
        req.minutes.unwrap_or(current.minutes),
        req.tz.as_deref().unwrap_or(&current.tz),
        req.repeat_days.unwrap_or(current.repeat_days),
        until.as_deref(),
    )?;
    sqlx::query(
        "UPDATE calendar_events SET project_id = ?, title = ?, start_local = ?, minutes = ?, tz = ?, repeat_days = ?,
            repeat_until = ?, first_start_at = ?, last_end_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&project_id)
    .bind(&title)
    .bind(times.start.format(LOCAL_FORMAT).to_string())
    .bind(times.minutes)
    .bind(times.tz.name())
    .bind(times.repeat_days as i64)
    .bind(times.until.map(|u| u.format("%Y-%m-%d").to_string()))
    .bind(time::format(times.first_start()))
    .bind(times.last_end().map(time::format))
    .bind(time::now())
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    let event = visible_event(&mut tx, &user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(event))
}

/// Delete an event, or with `occurrence`, take one week out of a repeat.
#[utoipa::path(delete, path = "/calendar/events/{id}", tag = "calendar",
    params(("id" = String, Path), DeleteQuery),
    responses((status = 204), (status = 400, body = crate::error::ErrorBody),
        (status = 403, body = crate::error::ErrorBody), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn delete_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(q): Query<DeleteQuery>,
) -> AppResult<StatusCode> {
    let mut tx = state.db.begin().await?;
    let event = editable_event(&mut tx, &user.id, &id).await?;
    match q.occurrence {
        Some(day) if event.repeat_days != 0 => {
            one_occurrence(&mut tx, &user.id, &event, &day).await?;
            sqlx::query("INSERT OR IGNORE INTO calendar_skips (event_id, occurrence) VALUES (?, ?)")
                .bind(&id)
                .bind(&day)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM session_joins WHERE event_id = ? AND occurrence = ?")
                .bind(&id)
                .bind(&day)
                .execute(&mut *tx)
                .await?;
        }
        _ => {
            sqlx::query("DELETE FROM calendar_events WHERE id = ?").bind(&id).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Say you'll join a session that week. Anyone who can see the project can.
#[utoipa::path(put, path = "/calendar/events/{id}/joins/{occurrence}", tag = "calendar",
    params(("id" = String, Path), ("occurrence" = String, Path, description = "Local date, YYYY-MM-DD")),
    responses((status = 200, body = Occurrence), (status = 400, body = crate::error::ErrorBody),
        (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn join_session(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, day)): Path<(String, String)>,
) -> AppResult<Json<Occurrence>> {
    let mut tx = state.db.begin().await?;
    let event = visible_event(&mut tx, &user.id, &id).await?;
    if event.kind != "session" {
        return Err(AppError::NotFound);
    }
    if event.user_id == user.id {
        return Err(bad("own_session", "You planned this session, so you're already on it."));
    }
    let before = one_occurrence(&mut tx, &user.id, &event, &day).await?;
    if !before.joined {
        sqlx::query(
            "INSERT INTO session_joins (event_id, occurrence, user_id, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&day)
        .bind(&user.id)
        .bind(time::now())
        .execute(&mut *tx)
        .await?;
        let start = parse_instant(&before.start_at, "start")?;
        log_session(&mut tx, &event, &user.id, "session.joined", start).await?;
    }
    let after = one_occurrence(&mut tx, &user.id, &event, &day).await?;
    tx.commit().await?;
    Ok(Json(after))
}

/// Take back saying you'll join.
#[utoipa::path(delete, path = "/calendar/events/{id}/joins/{occurrence}", tag = "calendar",
    params(("id" = String, Path), ("occurrence" = String, Path, description = "Local date, YYYY-MM-DD")),
    responses((status = 200, body = Occurrence), (status = 404, body = crate::error::ErrorBody)))]
pub(crate) async fn leave_session(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, day)): Path<(String, String)>,
) -> AppResult<Json<Occurrence>> {
    let mut tx = state.db.begin().await?;
    let event = visible_event(&mut tx, &user.id, &id).await?;
    sqlx::query("DELETE FROM session_joins WHERE event_id = ? AND occurrence = ? AND user_id = ?")
        .bind(&id)
        .bind(&day)
        .bind(&user.id)
        .execute(&mut *tx)
        .await?;
    let after = one_occurrence(&mut tx, &user.id, &event, &day).await?;
    tx.commit().await?;
    Ok(Json(after))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn weekday_repeats_keep_local_time_across_the_clock_change() {
        // Work 9 to 5 on weekdays in London, starting the week the clocks go back.
        let t = Times::new("2026-10-19T09:00", 480, "Europe/London", 31, None).unwrap();
        let got = t.between(utc("2026-10-19T00:00:00Z"), utc("2026-11-02T00:00:00Z"));
        assert_eq!(got.len(), 10);
        // Summer time: 09:00 local is 08:00 UTC.
        assert_eq!(time::format(got[0].1), "2026-10-19T08:00:00Z");
        // After 25 October it's 09:00 UTC.
        let after = got.iter().find(|(d, _, _)| d.to_string() == "2026-10-26").unwrap();
        assert_eq!(time::format(after.1), "2026-10-26T09:00:00Z");
        assert_eq!(time::format(after.2), "2026-10-26T17:00:00Z");
    }

    #[test]
    fn repeats_stop_at_their_end_date_and_once_means_once() {
        let t = Times::new("2026-11-14T13:00", 240, "UTC", 32, Some("2026-11-21")).unwrap();
        let got = t.between(utc("2026-11-01T00:00:00Z"), utc("2026-12-31T00:00:00Z"));
        let days: Vec<String> = got.iter().map(|(d, _, _)| d.to_string()).collect();
        assert_eq!(days, ["2026-11-14", "2026-11-21"]);
        let once = Times::new("2026-11-14T13:00", 60, "UTC", 0, Some("2026-12-01")).unwrap();
        assert!(once.until.is_none());
        assert_eq!(once.between(utc("2026-11-01T00:00:00Z"), utc("2026-12-31T00:00:00Z")).len(), 1);
        assert!(once.last_end().is_some());
    }

    #[test]
    fn a_block_from_the_day_before_reaches_into_the_range() {
        let t = Times::new("2026-11-08T22:00", 240, "UTC", 0, None).unwrap();
        let got = t.between(utc("2026-11-09T00:00:00Z"), utc("2026-11-10T00:00:00Z"));
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn checks_and_words() {
        assert!(Times::new("2026-11-14 13:00", 60, "UTC", 0, None).is_err());
        assert!(Times::new("2026-11-14T13:00", 10, "UTC", 0, None).is_err());
        assert!(Times::new("2026-11-14T13:00", 60, "Mars/Olympus", 0, None).is_err());
        assert!(Times::new("2026-11-14T13:00", 60, "UTC", 32, Some("2026-11-01")).is_err());
        assert_eq!(parse_days("weekdays").unwrap(), 31);
        assert_eq!(parse_days("Sat, sunday").unwrap(), 96);
        assert!(parse_days("someday").is_err());
        assert_eq!(days_text(31), "weekdays");
        assert_eq!(days_text(5), "Mon, Wed");
        assert_eq!(hours(90), "1h 30m");
        assert_eq!(hours(240), "4h");
    }
}
