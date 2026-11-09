//! Slows down password guessing. Each email gets a few free wrong passwords;
//! after that it must wait before the next try, twice as long each time, up
//! to fifteen minutes. A right password clears it. Kept in memory: a restart
//! forgets, which is fine for something that only has to cost a guesser time.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Wrong passwords allowed before the waits start.
const FREE_TRIES: u32 = 5;
const FIRST_WAIT: Duration = Duration::from_secs(30);
const LONGEST_WAIT: Duration = Duration::from_secs(15 * 60);
/// An email with no wrong password for this long starts again.
const FORGET_AFTER: Duration = Duration::from_secs(60 * 60);
/// Most emails remembered, so made-up ones can't fill memory.
const MAX_KEYS: usize = 10_000;

#[derive(Default)]
pub struct LoginLimiter {
    entries: Mutex<HashMap<String, Entry>>,
}

struct Entry {
    failures: u32,
    last: Instant,
    blocked_until: Option<Instant>,
}

impl LoginLimiter {
    /// `Err(seconds)` when `key` has to wait before trying again.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let now = Instant::now();
        let entries = self.entries.lock().unwrap();
        match entries.get(key).and_then(|e| e.blocked_until) {
            Some(until) if until > now => Err((until - now).as_secs().max(1)),
            _ => Ok(()),
        }
    }

    pub fn failed(&self, key: &str) {
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= MAX_KEYS && !entries.contains_key(key) {
            entries.retain(|_, e| now.duration_since(e.last) < FORGET_AFTER);
            if entries.len() >= MAX_KEYS {
                // Still full of fresh guesses: drop the oldest half.
                let mut ages: Vec<Instant> = entries.values().map(|e| e.last).collect();
                ages.sort_unstable();
                let cut = ages[ages.len() / 2];
                entries.retain(|_, e| e.last > cut);
            }
        }
        let e =
            entries.entry(key.to_string()).or_insert(Entry { failures: 0, last: now, blocked_until: None });
        if now.duration_since(e.last) >= FORGET_AFTER {
            e.failures = 0;
        }
        e.failures += 1;
        e.last = now;
        if e.failures >= FREE_TRIES {
            let doublings = (e.failures - FREE_TRIES).min(10);
            e.blocked_until = Some(now + (FIRST_WAIT * 2u32.pow(doublings)).min(LONGEST_WAIT));
        }
    }

    pub fn succeeded(&self, key: &str) {
        self.entries.lock().unwrap().remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_start_after_the_free_tries() {
        let l = LoginLimiter::default();
        for _ in 0..FREE_TRIES - 1 {
            l.failed("a@b.c");
            assert!(l.check("a@b.c").is_ok());
        }
        l.failed("a@b.c");
        let wait = l.check("a@b.c").unwrap_err();
        assert!((1..=30).contains(&wait), "{wait}");
        assert!(l.check("other@b.c").is_ok());
        l.succeeded("a@b.c");
        assert!(l.check("a@b.c").is_ok());
    }
}
