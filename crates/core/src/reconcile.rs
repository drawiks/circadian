use chrono::{DateTime, Utc};

use crate::model::{GapDraft, PersistedState, PowerSource, SessionDraft, SessionKind, Settings};
use crate::time::{local_offset_minutes, minutes_between};

#[derive(Debug, Clone, Copy)]
pub struct StartContext {
    pub now: DateTime<Utc>,
    pub boot_hint_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ReconcileOutcome {
    pub session: Option<SessionDraft>,
    pub gap: Option<GapDraft>,
    pub state: PersistedState,
}

pub fn reconcile_start(
    prev: &PersistedState,
    settings: &Settings,
    ctx: StartContext,
) -> ReconcileOutcome {
    let now = ctx.now;
    let mut state = prev.clone();
    let mut session = None;
    let mut gap = None;
    let mut awake_since = now;

    if let Some(pending) = prev.pending_power.clone() {
        let paused = prev
            .paused_until_utc
            .map(|until| until > pending.at_utc)
            .unwrap_or(false);
        let gap_minutes = minutes_between(pending.at_utc, now);

        if !paused {
            let start = pending.start_hint_utc.unwrap_or(pending.at_utc);
            let end = wake_time(now, ctx.boot_hint_utc, start);
            if pending.source == PowerSource::Manual {
                session = manual_session(start, end);
            } else if gap_minutes >= settings.ignore_below_minutes {
                session = classify(start, end, pending.source, settings);
            }
            awake_since = end;
        }
    } else if let Some(manual_start) = prev.manual_sleep_start_utc {
        let end = wake_time(now, ctx.boot_hint_utc, manual_start);
        if end > manual_start {
            session = manual_session(manual_start, end);
            awake_since = end;
        }
    } else if let Some(last_seen) = prev.last_seen_utc {
        let same_boot = match (prev.last_boot_hint_utc, ctx.boot_hint_utc) {
            (Some(previous), Some(current)) => (current - previous).num_seconds().abs() <= 60,
            _ => true,
        };
        let gap_minutes = minutes_between(last_seen, now);

        if !same_boot && gap_minutes >= settings.ignore_below_minutes {
            let end = wake_time(now, ctx.boot_hint_utc, last_seen);
            session = classify(last_seen, end, PowerSource::Boot, settings);
            awake_since = end;
        } else if same_boot && gap_minutes >= settings.ignore_below_minutes {
            gap = Some(GapDraft {
                start_utc: last_seen,
                end_utc: now,
                reason: "демон не работал".to_string(),
            });
            awake_since = prev.awake_since_utc.unwrap_or(now);
        } else {
            awake_since = prev.awake_since_utc.unwrap_or(now);
        }
    }

    state.last_seen_utc = Some(now);
    if ctx.boot_hint_utc.is_some() {
        state.last_boot_hint_utc = ctx.boot_hint_utc;
    }
    state.pending_power = None;
    state.manual_sleep_start_utc = None;
    state.awake_since_utc = Some(awake_since);
    if state
        .paused_until_utc
        .map(|until| until <= now)
        .unwrap_or(false)
    {
        state.paused_until_utc = None;
    }

    ReconcileOutcome {
        session,
        gap,
        state,
    }
}

fn wake_time(
    now: DateTime<Utc>,
    boot_hint: Option<DateTime<Utc>>,
    start: DateTime<Utc>,
) -> DateTime<Utc> {
    match boot_hint {
        Some(boot) if boot >= start && boot <= now => boot,
        _ => now,
    }
}

fn manual_session(start: DateTime<Utc>, end: DateTime<Utc>) -> Option<SessionDraft> {
    if end <= start {
        return None;
    }
    Some(SessionDraft {
        start_utc: start,
        end_utc: end,
        start_offset_min: local_offset_minutes(start),
        end_offset_min: local_offset_minutes(end),
        kind: SessionKind::Sleep,
        source: PowerSource::Manual,
        confidence: 100,
        confirmed: true,
        note: String::new(),
    })
}

fn classify(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    source: PowerSource,
    settings: &Settings,
) -> Option<SessionDraft> {
    if end <= start {
        return None;
    }
    if matches!(source, PowerSource::Suspend | PowerSource::Hibernate)
        && !settings.suspend_counts_as_sleep
    {
        return None;
    }

    let minutes = minutes_between(start, end);
    if minutes < settings.ignore_below_minutes {
        return None;
    }
    if minutes < settings.pending_below_minutes {
        return None;
    }

    let confirmed = minutes >= settings.auto_confirm_minutes;
    let confidence = if confirmed {
        match source {
            PowerSource::Shutdown => 95,
            PowerSource::Suspend | PowerSource::Hibernate => 75,
            PowerSource::Boot => 80,
            PowerSource::Manual => 100,
        }
    } else {
        50
    };

    Some(SessionDraft {
        start_utc: start,
        end_utc: end,
        start_offset_min: local_offset_minutes(start),
        end_offset_min: local_offset_minutes(end),
        kind: SessionKind::Sleep,
        source,
        confidence,
        confirmed,
        note: String::new(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    fn at(hour: i64, minute: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 22, hour as u32, minute as u32, 0)
            .unwrap()
    }

    fn settings() -> Settings {
        Settings::default()
    }

    #[test]
    fn ignores_quick_reboot() {
        let now = at(10, 0);
        let mut prev = PersistedState::default();
        prev.last_seen_utc = Some(now - Duration::hours(8));
        prev.last_boot_hint_utc = Some(now - Duration::hours(12));
        prev.pending_power = Some(crate::model::PendingPower {
            source: PowerSource::Shutdown,
            at_utc: now - Duration::minutes(3),
            start_hint_utc: None,
        });
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(now - Duration::minutes(2)),
            },
        );
        assert!(result.session.is_none());
        assert!(result.gap.is_none());
        assert_eq!(result.state.pending_power, None);
    }

    #[test]
    fn shutdown_becomes_sleep() {
        let now = at(10, 0);
        let mut prev = PersistedState::default();
        prev.last_seen_utc = Some(now - Duration::hours(8));
        prev.last_boot_hint_utc = Some(now - Duration::hours(30));
        prev.pending_power = Some(crate::model::PendingPower {
            source: PowerSource::Shutdown,
            at_utc: now - Duration::hours(8),
            start_hint_utc: None,
        });
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(now - Duration::minutes(2)),
            },
        );
        let session = result.session.expect("sleep session");
        assert_eq!(session.duration_minutes(), 8 * 60 - 2);
        assert!(session.confirmed);
        assert_eq!(session.source, PowerSource::Shutdown);
    }

    #[test]
    fn daytime_absence_waits_for_confirmation() {
        let now = at(18, 0);
        let mut prev = PersistedState::default();
        prev.last_seen_utc = Some(now - Duration::hours(2));
        prev.pending_power = Some(crate::model::PendingPower {
            source: PowerSource::Shutdown,
            at_utc: now - Duration::hours(2),
            start_hint_utc: None,
        });
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(now - Duration::minutes(3)),
            },
        );
        let session = result.session.expect("pending session");
        assert!(!session.confirmed);
        assert_eq!(session.kind, SessionKind::Sleep);
    }

    #[test]
    fn daemon_downtime_is_not_sleep() {
        let now = at(20, 0);
        let mut prev = PersistedState::default();
        prev.last_seen_utc = Some(now - Duration::hours(3));
        prev.last_boot_hint_utc = Some(now - Duration::hours(5));
        prev.awake_since_utc = Some(now - Duration::hours(6));
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(now - Duration::hours(5) + Duration::seconds(1)),
            },
        );
        assert!(result.session.is_none());
        assert!(result.gap.is_some());
        assert_eq!(result.state.awake_since_utc, Some(now - Duration::hours(6)));
    }

    #[test]
    fn power_loss_uses_last_seen() {
        let now = at(9, 0);
        let mut prev = PersistedState::default();
        prev.last_seen_utc = Some(now - Duration::hours(7));
        prev.last_boot_hint_utc = Some(now - Duration::hours(20));
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(now - Duration::minutes(2)),
            },
        );
        let session = result.session.expect("boot session");
        assert_eq!(session.source, PowerSource::Boot);
        assert!(session.confirmed);
        assert_eq!(session.duration_minutes(), 7 * 60 - 2);
    }

    #[test]
    fn late_daemon_start_uses_boot_time() {
        let now = at(13, 0);
        let boot = now - Duration::hours(4);
        let mut prev = PersistedState::default();
        prev.last_seen_utc = Some(now - Duration::hours(8));
        prev.last_boot_hint_utc = Some(now - Duration::hours(30));
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(boot),
            },
        );
        let session = result.session.expect("boot session");
        assert_eq!(session.end_utc, boot);
        assert_eq!(session.duration_minutes(), 4 * 60);
        assert_eq!(result.state.awake_since_utc, Some(boot));
    }

    #[test]
    fn manual_sleep_is_confirmed() {
        let now = at(12, 0);
        let mut prev = PersistedState::default();
        prev.manual_sleep_start_utc = Some(now - Duration::hours(1));
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: None,
            },
        );
        let session = result.session.expect("manual session");
        assert!(session.confirmed);
        assert_eq!(session.source, PowerSource::Manual);
    }

    #[test]
    fn pause_suppresses_session() {
        let now = at(12, 0);
        let mut prev = PersistedState::default();
        prev.pending_power = Some(crate::model::PendingPower {
            source: PowerSource::Shutdown,
            at_utc: now - Duration::hours(8),
            start_hint_utc: None,
        });
        prev.paused_until_utc = Some(now + Duration::hours(1));
        let result = reconcile_start(
            &prev,
            &settings(),
            StartContext {
                now,
                boot_hint_utc: Some(now - Duration::minutes(2)),
            },
        );
        assert!(result.session.is_none());
    }
}
