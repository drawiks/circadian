use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike, Utc};

const WEEKDAYS: [&str; 7] = ["пн", "вт", "ср", "чт", "пт", "сб", "вс"];
const MONTHS: [&str; 12] = [
    "янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
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

pub fn weekday_ru(date: NaiveDate) -> &'static str {
    WEEKDAYS[date.weekday().num_days_from_monday() as usize]
}

pub fn month_ru(month: u32) -> &'static str {
    MONTHS[(month.saturating_sub(1) as usize).min(11)]
}

pub fn format_clock(at: DateTime<Utc>, offset_min: i32) -> String {
    local_datetime(at, offset_min).format("%H:%M").to_string()
}

pub fn format_time_local(value: NaiveDateTime) -> String {
    value.format("%H:%M").to_string()
}

pub fn format_date_short(date: NaiveDate) -> String {
    format!("{} {}", date.day(), month_ru(date.month()))
}

pub fn format_date_ru(date: NaiveDate) -> String {
    format!(
        "{}, {} {}",
        weekday_ru(date),
        date.day(),
        month_ru(date.month())
    )
}

pub fn format_date_full(date: NaiveDate) -> String {
    format!("{:02}.{:02}.{}", date.day(), date.month(), date.year())
}

pub fn format_datetime_ru(value: NaiveDateTime) -> String {
    format!(
        "{}, {} {} · {}",
        weekday_ru(value.date()),
        value.date().day(),
        month_ru(value.date().month()),
        value.format("%H:%M")
    )
}

pub fn format_duration(minutes: i64) -> String {
    let minutes = minutes.max(0);
    let hours = minutes / 60;
    let rest = minutes % 60;
    if hours == 0 {
        format!("{rest} мин")
    } else if rest == 0 {
        format!("{hours} ч")
    } else {
        format!("{hours} ч {rest:02} мин")
    }
}

pub fn format_signed_duration(minutes: i64) -> String {
    if minutes == 0 {
        return "0 мин".to_string();
    }
    let sign = if minutes > 0 { "+" } else { "−" };
    format!("{sign}{}", format_duration(minutes.abs()))
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
        assert_eq!(format_duration(0), "0 мин");
        assert_eq!(format_duration(45), "45 мин");
        assert_eq!(format_duration(120), "2 ч");
        assert_eq!(format_duration(438), "7 ч 18 мин");
        assert_eq!(format_signed_duration(-80), "−1 ч 20 мин");
        assert_eq!(format_signed_duration(25), "+25 мин");
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
}
