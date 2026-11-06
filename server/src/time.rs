use chrono::{DateTime, SecondsFormat, Utc};

/// Every timestamp in the database is RFC 3339 UTC with second precision and a
/// trailing `Z`, so string comparison in SQL matches time order.
pub fn format(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn now() -> String {
    format(Utc::now())
}

pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
