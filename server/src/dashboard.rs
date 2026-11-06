//! A project's overview: stat tiles, the weekly chart, what's next and what
//! happened lately. One call so the page draws at once.

use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{Datelike, Duration, NaiveDate, Utc};
use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    AppState,
    auth::CurrentUser,
    error::AppResult,
    projects::visible_project,
    tasks::{ACTIVITY_SELECT, ActivityItem, Task, project_tasks},
};

const WEEKS: usize = 8;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(project_dashboard))
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct ColumnCount {
    pub status_id: String,
    pub name: String,
    /// `todo`, `in_progress`, `blocked` or `done`.
    pub category: String,
    pub color: Option<String>,
    pub count: i64,
}

#[derive(Serialize, ToSchema)]
pub struct WeekCount {
    /// The Monday the week starts on, `YYYY-MM-DD`.
    pub week: String,
    pub done: i64,
}

#[derive(Serialize, ToSchema)]
pub struct Dashboard {
    /// Every column in board order, with how many tasks sit in it.
    pub columns: Vec<ColumnCount>,
    /// Tasks finished in each of the last eight weeks, oldest first.
    pub weekly: Vec<WeekCount>,
    /// Up to five open tasks: late and soonest due first, then by priority.
    pub up_next: Vec<Task>,
    /// The latest fifteen things that happened.
    pub activity: Vec<ActivityItem>,
}

fn week_start(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// Buckets completion dates into the `WEEKS` weeks ending with `today`'s.
fn weekly_counts(today: NaiveDate, completed: &[String]) -> Vec<WeekCount> {
    let this_week = week_start(today);
    let starts: Vec<NaiveDate> = (0..WEEKS).rev().map(|i| this_week - Duration::weeks(i as i64)).collect();
    let mut counts = vec![0i64; WEEKS];
    for c in completed {
        let Some(day) = c.get(..10).and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) else {
            continue;
        };
        if let Some(i) = starts.iter().position(|s| *s == week_start(day)) {
            counts[i] += 1;
        }
    }
    starts.into_iter().zip(counts).map(|(s, done)| WeekCount { week: s.to_string(), done }).collect()
}

fn priority_rank(p: Option<&str>) -> u8 {
    match p {
        Some("urgent") => 0,
        Some("high") => 1,
        Some("medium") => 2,
        Some("low") => 3,
        _ => 4,
    }
}

/// Stat tiles, weekly chart, up next and activity for a project.
#[utoipa::path(get, path = "/projects/{id}/dashboard", tag = "tasks", params(("id" = String, Path)),
    responses((status = 200, body = Dashboard), (status = 404, body = crate::error::ErrorBody)))]
async fn project_dashboard(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> AppResult<Json<Dashboard>> {
    let mut conn = state.db.acquire().await?;
    visible_project(&mut conn, &user.id, &id).await?;

    let columns = sqlx::query_as::<_, ColumnCount>(
        "SELECT s.id AS status_id, s.name, s.category, s.color,
                (SELECT COUNT(*) FROM tasks t WHERE t.status_id = s.id) AS count
         FROM statuses s WHERE s.project_id = ? ORDER BY s.position",
    )
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;

    let today = Utc::now().date_naive();
    let since = week_start(today) - Duration::weeks(WEEKS as i64 - 1);
    let completed: Vec<String> =
        sqlx::query_scalar("SELECT completed_at FROM tasks WHERE project_id = ? AND completed_at >= ?")
            .bind(&id)
            .bind(since.to_string())
            .fetch_all(&mut *conn)
            .await?;
    let weekly = weekly_counts(today, &completed);

    let mut open: Vec<Task> =
        project_tasks(&mut conn, &id).await?.into_iter().filter(|t| t.category != "done").collect();
    open.sort_by(|a, b| {
        let due = |t: &Task| (t.due_date.is_none(), t.due_date.clone());
        due(a)
            .cmp(&due(b))
            .then(priority_rank(a.priority.as_deref()).cmp(&priority_rank(b.priority.as_deref())))
            .then((a.category != "in_progress").cmp(&(b.category != "in_progress")))
    });
    open.truncate(5);

    let activity = sqlx::query_as::<_, ActivityItem>(&format!(
        "{ACTIVITY_SELECT} WHERE a.project_id = ? ORDER BY a.created_at DESC, a.id DESC LIMIT 15"
    ))
    .bind(&id)
    .fetch_all(&mut *conn)
    .await?;

    Ok(Json(Dashboard { columns, weekly, up_next: open, activity }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_by_monday() {
        // 2026-11-06 is a Friday; its week starts Monday 2026-11-02.
        let today = NaiveDate::from_ymd_opt(2026, 11, 6).unwrap();
        let done = vec![
            "2026-11-02T08:00:00Z".to_string(),
            "2026-11-06T23:00:00Z".to_string(),
            "2026-11-01T12:00:00Z".to_string(),
            "2026-01-01T12:00:00Z".to_string(),
        ];
        let weeks = weekly_counts(today, &done);
        assert_eq!(weeks.len(), 8);
        assert_eq!(weeks[7].week, "2026-11-02");
        assert_eq!(weeks[7].done, 2);
        assert_eq!(weeks[6].week, "2026-10-26");
        assert_eq!(weeks[6].done, 1);
        assert_eq!(weeks[0].week, "2026-09-14");
        assert_eq!(weeks.iter().map(|w| w.done).sum::<i64>(), 3);
    }
}
