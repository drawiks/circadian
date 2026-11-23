#![allow(clippy::field_reassign_with_default)]

use chrono::{TimeZone, Utc};
use circadian_core::model::{
    GapDraft, PersistedState, PowerSource, SessionDraft, SessionKind, Settings,
};
use circadian_core::Store;

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "circadian-{name}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ))
}

fn draft(start_hour: u32, end_hour: u32) -> SessionDraft {
    SessionDraft {
        start_utc: Utc
            .with_ymd_and_hms(2026, 11, 22, start_hour, 0, 0)
            .unwrap(),
        end_utc: Utc.with_ymd_and_hms(2026, 11, 22, end_hour, 0, 0).unwrap(),
        start_offset_min: 0,
        end_offset_min: 0,
        kind: SessionKind::Sleep,
        source: PowerSource::Shutdown,
        confidence: 95,
        confirmed: true,
        note: String::new(),
    }
}

#[test]
fn stores_sessions_settings_state_and_gaps() {
    let dir = temp_dir("store");
    let path = dir.join("circadian.db");
    let store = Store::open(&path).expect("open store");

    assert_eq!(store.load_settings().unwrap(), Settings::default());
    let mut settings = Settings::default();
    settings.target_sleep_minutes = 420;
    settings.theme = circadian_core::model::Theme::Light;
    store.save_settings(&settings).unwrap();
    assert_eq!(store.load_settings().unwrap(), settings);

    let id = store.insert_session(&draft(1, 8)).expect("insert");
    assert!(store
        .overlaps(draft(2, 9).start_utc, draft(2, 9).end_utc, None)
        .unwrap());
    assert!(store.try_insert_session(&draft(2, 9)).unwrap().is_none());
    let second = store.insert_session(&draft(9, 10)).expect("second insert");
    let sessions = store.all_sessions().unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].duration_minutes(), 60);

    store.set_kind(id, SessionKind::Away, true).unwrap();
    assert_eq!(
        store.session_by_id(id).unwrap().unwrap().kind,
        SessionKind::Away
    );
    store.update_session(id, &draft(0, 8)).unwrap();
    assert_eq!(
        store.session_by_id(id).unwrap().unwrap().duration_minutes(),
        8 * 60
    );
    store.delete_session(second).unwrap();
    assert_eq!(store.all_sessions().unwrap().len(), 1);
    assert!(store.update_session(999, &draft(1, 2)).is_err());

    let mut state = PersistedState::default();
    state.last_seen_utc = Some(Utc.with_ymd_and_hms(2026, 11, 22, 12, 0, 0).unwrap());
    state.awake_since_utc = Some(Utc.with_ymd_and_hms(2026, 11, 22, 8, 0, 0).unwrap());
    store.save_state(&state).unwrap();
    assert_eq!(store.load_state().unwrap(), state);

    let gap = GapDraft {
        start_utc: Utc.with_ymd_and_hms(2026, 11, 22, 13, 0, 0).unwrap(),
        end_utc: Utc.with_ymd_and_hms(2026, 11, 22, 14, 0, 0).unwrap(),
        reason: "демон не работал".to_string(),
    };
    assert!(store.insert_gap_if_new(&gap).unwrap());
    assert!(!store.insert_gap_if_new(&gap).unwrap());
    assert_eq!(
        store
            .gaps_between(gap.start_utc, gap.end_utc + chrono::Duration::minutes(1))
            .unwrap()
            .len(),
        1
    );

    drop(store);
    let _ = std::fs::remove_dir_all(dir);
}
