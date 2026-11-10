//! The Today screen: what's in progress, what's stuck, what's due, today's
//! sessions, projects that have gone quiet, and the Inbox.

use axum::{
    Json,
    extract::{Query, State},
};
use chrono::{Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::{CurrentUser, User},
    calendar::{self, Occurrence},
    error::AppResult,
    notes::{self, NoteSummary},
    tasks::{self, Task},
};

/// An active project with no activity for this many days counts as gone quiet.
pub const QUIET_DAYS: i64 = 14;
/// How far ahead "due" looks.
pub const DUE_DAYS: i64 = 7;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(today))
}

#[derive(Deserialize, IntoParams)]
pub struct TodayQuery {
    /// The viewer's IANA time zone, such as Europe/London, which says when
    /// today starts. Defaults to the zone they last planned time in.
    pub tz: Option<String>,
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct QuietProject {
    pub id: String,
    pub title: String,
    pub color: String,
    /// Days since anything happened on it.
    pub quiet_days: i64,
    pub total_tasks: i64,
    pub open_tasks: i64,
    /// When anything last happened on it.
    pub last_active_at: String,
}

#[derive(Serialize, ToSchema)]
pub struct Today {
    /// Today in the viewer's time zone, `YYYY-MM-DD`.
    pub date: String,
    /// In progress, yours or nobody's.
    pub now: Vec<Task>,
    pub blocked: Vec<Task>,
    /// Open tasks due in the next 7 days, and late ones, soonest first.
    pub due: Vec<Task>,
    /// Today's work sessions: yours and those you're joining.
    pub sessions: Vec<Occurrence>,
    /// Your active projects with no activity for 14 days, quietest first.
    pub quiet: Vec<QuietProject>,
    /// Quick captures not filed under a project yet, newest first.
    pub inbox: Vec<NoteSummary>,
}

/// What to touch next, across every project.
#[utoipa::path(get, path = "/today", tag = "today", params(TodayQuery),
    responses((status = 200, body = Today), (status = 400, body = crate::error::ErrorBody)))]
pub(crate) async fn today(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(q): Query<TodayQuery>,
) -> AppResult<Json<Today>> {
    Ok(Json(build(&state, &user, q.tz.as_deref()).await?))
}

pub async fn build(state: &AppState, user: &User, tz: Option<&str>) -> AppResult<Today> {
    let mut conn = state.db.acquire().await?;
    let tz = match tz.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => calendar::parse_tz(t)?,
        None => calendar::parse_tz(&calendar::usual_tz(&mut conn, &user.id).await?).unwrap_or(chrono_tz::UTC),
    };
    let local_today: NaiveDate = Utc::now().with_timezone(&tz).date_naive();
    let until = (local_today + Duration::days(DUE_DAYS)).format("%Y-%m-%d").to_string();
    let open = tasks::today_tasks(&mut conn, &user.id, &until).await?;
    let now = open.iter().filter(|t| t.category == "in_progress").cloned().collect();
    let blocked = open.iter().filter(|t| t.category == "blocked").cloned().collect();
    let due =
        open.into_iter().filter(|t| t.due_date.as_deref().is_some_and(|d| d <= until.as_str())).collect();

    let start = calendar::to_utc(tz, local_today.and_hms_opt(0, 0, 0).expect("midnight exists"));
    let next = local_today.succ_opt().unwrap_or(local_today);
    let end = calendar::to_utc(tz, next.and_hms_opt(0, 0, 0).expect("midnight exists"));
    let sessions = calendar::occurrences_between(&mut conn, &user.id, start, end, None, true)
        .await?
        .into_iter()
        .filter(|o| o.event.kind == "session")
        .collect();

    let cutoff = crate::time::format(Utc::now() - Duration::days(QUIET_DAYS));
    let quiet = sqlx::query_as::<_, QuietProject>(
        "SELECT p.id, p.title, p.color,
            CAST(julianday('now') - julianday(st.last_active_at) AS INTEGER) AS quiet_days,
            st.total_tasks, st.open_tasks, st.last_active_at
         FROM projects p JOIN project_stats st ON st.project_id = p.id
         WHERE p.owner_id = ? AND p.status = 'active'
           AND st.last_active_at < ?
         ORDER BY st.last_active_at LIMIT 20",
    )
    .bind(&user.id)
    .bind(&cutoff)
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    let Json(inbox) = notes::list_notes(
        State(state.clone()),
        CurrentUser(user.clone()),
        Query(notes::ListNotesQuery {
            q: None,
            tag: None,
            project_id: None,
            folder: Some(crate::notes::vault::INBOX.into()),
            shared: Some(false),
            limit: Some(50),
        }),
    )
    .await?;
    let inbox = inbox.into_iter().filter(|n| n.projects.is_empty()).take(20).collect();
    Ok(Today { date: local_today.format("%Y-%m-%d").to_string(), now, blocked, due, sessions, quiet, inbox })
}
