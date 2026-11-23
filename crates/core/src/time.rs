use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike, Utc};

use crate::i18n::Lang;

const WEEKDAYS_RU: [&str; 7] = ["пн", "вт", "ср", "чт", "пт", "сб", "вс"];
const WEEKDAYS_EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS_RU: [&str; 12] = [
    "янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
];
const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub fn now_utc() -> DateTime<Utc> {
    Utc::now()
}

pub fn local_offset_minutes(at: DateTime<Utc>) -> i32 {
    at.with_timezone(&Local).offset().local_minus_utc() / 60
}

pub fn local_datetime(at: DateTime<Utc>, offset_min: i32) -> NaiveDateTime {
    let offset = chrono::FixedOffset::east_opt(offset_min * 60)
        .unwrap_or_else(|| chrono::FixedOffset::east_opt(0).expect("zero offset is valid"));
    at.with_timezone(&offset).naive_local()
}

pub fn local_date(at: DateTime<Utc>, offset_min: i32) -> NaiveDate {
    local_datetime(at, offset_min).date()
}

pub fn weekday(date: NaiveDate, lang: Lang) -> &'static str {
    let index = date.weekday().num_days_from_monday() as usize;
    if lang.is_russian() {
        WEEKDAYS_RU[index]
    } else {
        WEEKDAYS_EN[index]
    }
}

pub fn month(month: u32, lang: Lang) -> &'static str {
    let index = (month.saturating_sub(1) as usize).min(11);
    if lang.is_russian() {
        MONTHS_RU[index]
    } else {
        MONTHS_EN[index]
    }
}

pub fn format_clock(at: DateTime<Utc>, offset_min: i32) -> String {
    local_datetime(at, offset_min).format("%H:%M").to_string()
}

pub fn format_time_local(value: NaiveDateTime) -> String {
    value.format("%H:%M").to_string()
}

pub fn format_date_short(date: NaiveDate, lang: Lang) -> String {
    format!("{} {}", date.day(), month(date.month(), lang))
}

pub fn format_date(date: NaiveDate, lang: Lang) -> String {
    format!(
        "{}, {} {}",
        weekday(date, lang),
        date.day(),
        month(date.month(), lang)
    )
}

pub fn format_date_full(date: NaiveDate) -> String {
    format!("{:02}.{:02}.{}", date.day(), date.month(), date.year())
}

pub fn format_datetime(value: NaiveDateTime, lang: Lang) -> String {
    format!(
        "{}, {} {} · {}",
        weekday(value.date(), lang),
        value.date().day(),
        month(value.date().month(), lang),
        value.format("%H:%M")
    )
}

pub fn format_duration(minutes: i64, lang: Lang) -> String {
    let minutes = minutes.max(0);
    let hours = minutes / 60;
    let rest = minutes % 60;
    if lang.is_russian() {
        if hours == 0 {
            format!("{rest} мин")
        } else if rest == 0 {
            format!("{hours} ч")
        } else {
            format!("{hours} ч {rest:02} мин")
        }
    } else if hours == 0 {
        format!("{rest}m")
    } else if rest == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {rest:02}m")
    }
}

pub fn format_signed_duration(minutes: i64, lang: Lang) -> String {
    if minutes == 0 {
        return if lang.is_russian() { "0 мин" } else { "0m" }.to_string();
    }
    let sign = if minutes > 0 { "+" } else { "−" };
    format!("{sign}{}", format_duration(minutes.abs(), lang))
}

pub fn week_start(date: NaiveDate) -> NaiveDate {
    date - Duration::days(date.weekday().num_days_from_monday() as i64)
}

pub fn minutes_of_day(value: NaiveDateTime) -> f64 {
    value.time().num_seconds_from_midnight() as f64 / 60.0
}

pub fn minutes_between(start: DateTime<Utc>, end: DateTime<Utc>) -> i64 {
    (end - start).num_minutes().max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(0, Lang::Russian), "0 мин");
        assert_eq!(format_duration(45, Lang::Russian), "45 мин");
        assert_eq!(format_duration(120, Lang::Russian), "2 ч");
        assert_eq!(format_duration(438, Lang::Russian), "7 ч 18 мин");
        assert_eq!(format_signed_duration(-80, Lang::Russian), "−1 ч 20 мин");
        assert_eq!(format_signed_duration(25, Lang::Russian), "+25 мин");
        assert_eq!(format_duration(438, Lang::English), "7h 18m");
        assert_eq!(format_signed_duration(-80, Lang::English), "−1h 20m");
    }

    #[test]
    fn week_starts_monday() {
        let sunday = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        assert_eq!(
            week_start(sunday),
            NaiveDate::from_ymd_opt(2026, 11, 16).unwrap()
        );
        let monday = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap();
        assert_eq!(week_start(monday), monday);
    }

    #[test]
    fn local_date_uses_offset() {
        let at = Utc.with_ymd_and_hms(2026, 11, 22, 23, 30, 0).unwrap();
        assert_eq!(
            local_date(at, 120),
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
        );
        assert_eq!(format_clock(at, 120), "01:30");
    }

    #[test]
    fn formats_english_dates() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        assert_eq!(format_date(date, Lang::English), "Sun, 22 Nov");
        assert_eq!(format_date(date, Lang::Russian), "вс, 22 ноя");
    }
}
