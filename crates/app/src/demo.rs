use std::path::PathBuf;
use std::rc::Rc;

use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use circadian_core::i18n::{Lang, Language};
use circadian_core::model::{
    PersistedState, PowerSource, SessionDraft, SessionKind, Settings, Theme,
};
use circadian_core::time::{local_offset_minutes, week_start};
use circadian_core::Store;

pub struct Demo {
    pub store: Rc<Store>,
    pub data_dir: PathBuf,
    pub week_start: NaiveDate,
    pub theme: Theme,
}

pub fn create(lang: Lang, theme: Theme) -> Result<Demo, Box<dyn std::error::Error>> {
    let data_dir = std::env::temp_dir().join(format!(
        "circadian-demo-{}-{}",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    let store = Store::open(data_dir.join("circadian.db"))?;
    let now = Utc::now();
    let today = Local::now().date_naive();
    let previous_monday = week_start(today) - Duration::days(7);

    let rows = [
        (-1, 23, 30, 7, 10),
        (0, 2, 40, 9, 20),
        (1, 22, 50, 6, 30),
        (2, 3, 20, 11, 5),
        (3, 0, 40, 9, 10),
        (4, 4, 10, 13, 0),
        (5, 1, 30, 9, 40),
    ];
    for (day, start_hour, start_minute, end_hour, end_minute) in rows {
        let wake_date = previous_monday + Duration::days(day + 1);
        let start_date = if start_hour < end_hour {
            wake_date
        } else {
            wake_date - Duration::days(1)
        };
        let start = local_datetime(start_date, start_hour, start_minute);
        let end = local_datetime(wake_date, end_hour, end_minute);
        store.try_insert_session(&draft(
            start,
            end,
            SessionKind::Sleep,
            PowerSource::Shutdown,
        ))?;
    }

    let nap_start = local_datetime(previous_monday + Duration::days(2), 14, 0);
    let nap_end = local_datetime(previous_monday + Duration::days(2), 15, 10);
    store.try_insert_session(&draft(
        nap_start,
        nap_end,
        SessionKind::Nap,
        PowerSource::Manual,
    ))?;
    let away_start = local_datetime(previous_monday + Duration::days(5), 15, 0);
    let away_end = local_datetime(previous_monday + Duration::days(5), 17, 0);
    store.try_insert_session(&draft(
        away_start,
        away_end,
        SessionKind::Away,
        PowerSource::Shutdown,
    ))?;

    let recent_end = local_datetime(today, 6, 0);
    let recent_start = recent_end - Duration::hours(7);
    store.try_insert_session(&draft(
        recent_start,
        recent_end,
        SessionKind::Sleep,
        PowerSource::Shutdown,
    ))?;

    let settings = Settings {
        language: if lang.is_russian() {
            Language::Russian
        } else {
            Language::English
        },
        theme,
        autostart: false,
        ..Settings::default()
    };
    store.save_settings(&settings)?;
    let state = PersistedState {
        last_seen_utc: Some(now),
        last_boot_hint_utc: Some(now - Duration::hours(5)),
        pending_power: None,
        awake_since_utc: Some(recent_end),
        manual_sleep_start_utc: None,
        paused_until_utc: None,
    };
    store.save_state(&state)?;

    Ok(Demo {
        store: Rc::new(store),
        data_dir,
        week_start: previous_monday,
        theme,
    })
}

fn local_datetime(date: NaiveDate, hour: u32, minute: u32) -> DateTime<Utc> {
    let naive = date.and_hms_opt(hour, minute, 0).expect("valid demo time");
    Local
        .from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| {
            Local
                .from_local_datetime(&naive)
                .latest()
                .expect("valid local time")
        })
        .with_timezone(&Utc)
}

fn draft(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    kind: SessionKind,
    source: PowerSource,
) -> SessionDraft {
    SessionDraft {
        start_utc: start,
        end_utc: end,
        start_offset_min: local_offset_minutes(start),
        end_offset_min: local_offset_minutes(end),
        kind,
        source,
        confidence: 95,
        confirmed: true,
        note: String::new(),
    }
}
