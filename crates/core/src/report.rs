use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};

use crate::model::{Session, SessionKind};
use crate::time::{
    format_date_ru, format_duration, format_signed_duration, format_time_local, local_datetime,
    minutes_of_day, weekday_ru,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactKind {
    Summary,
    Trend,
    Debt,
    Longest,
    Shortest,
    Regularity,
    Nap,
    Time,
    Data,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    pub kind: FactKind,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaySummary {
    pub date: Option<NaiveDate>,
    pub total_min: i64,
    pub main_min: i64,
    pub nap_min: i64,
    pub sessions: usize,
    pub pending: usize,
    pub main_start: Option<NaiveDateTime>,
    pub main_end: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeekReport {
    pub week_start: NaiveDate,
    pub week_end: NaiveDate,
    pub days: Vec<DaySummary>,
    pub total_min: i64,
    pub avg_per_day_min: i64,
    pub days_with_data: usize,
    pub session_count: usize,
    pub nap_count: usize,
    pub nap_total_min: i64,
    pub debt_min: i64,
    pub longest: Option<(NaiveDate, i64)>,
    pub shortest: Option<(NaiveDate, i64)>,
    pub latest_start: Option<(NaiveDate, NaiveDateTime)>,
    pub earliest_end: Option<(NaiveDate, NaiveDateTime)>,
    pub regularity_sd_min: Option<i64>,
    pub previous_total_min: Option<i64>,
    pub facts: Vec<Fact>,
}

pub fn build_week_report(
    sessions: &[Session],
    previous_sessions: &[Session],
    week_start_date: NaiveDate,
    target_minutes: i64,
) -> WeekReport {
    let week_end = week_start_date + Duration::days(7);
    let mut by_day: BTreeMap<NaiveDate, Vec<&Session>> = BTreeMap::new();
    let mut pending_by_day: BTreeMap<NaiveDate, usize> = BTreeMap::new();

    for session in sessions {
        let date = local_datetime(session.end_utc, session.end_offset_min).date();
        if date < week_start_date || date >= week_end {
            continue;
        }
        if session.kind.counts_as_sleep() {
            if session.confirmed {
                by_day.entry(date).or_default().push(session);
            } else {
                *pending_by_day.entry(date).or_default() += 1;
            }
        }
    }

    let mut days = Vec::with_capacity(7);
    for index in 0..7 {
        let date = week_start_date + Duration::days(index);
        let day_sessions = by_day.get(&date).cloned().unwrap_or_default();
        let mut total = 0;
        let mut main: Option<&&Session> = None;
        for session in &day_sessions {
            let minutes = session.duration_minutes();
            total += minutes;
            if main
                .map(|current| current.duration_minutes() < minutes)
                .unwrap_or(true)
            {
                main = Some(session);
            }
        }
        let main_min = main.map(|session| session.duration_minutes()).unwrap_or(0);
        days.push(DaySummary {
            date: Some(date),
            total_min: total,
            main_min,
            nap_min: total - main_min,
            sessions: day_sessions.len(),
            pending: pending_by_day.get(&date).copied().unwrap_or(0),
            main_start: main
                .map(|session| local_datetime(session.start_utc, session.start_offset_min)),
            main_end: main.map(|session| local_datetime(session.end_utc, session.end_offset_min)),
        });
    }

    let total_min: i64 = days.iter().map(|day| day.total_min).sum();
    let days_with_data = days.iter().filter(|day| day.total_min > 0).count();
    let session_count = days.iter().map(|day| day.sessions).sum();
    let avg_per_day_min = if days_with_data == 0 {
        0
    } else {
        total_min / days_with_data as i64
    };
    let debt_min = target_minutes * days_with_data as i64 - total_min;
    let nap_count = sessions
        .iter()
        .filter(|session| {
            session.confirmed
                && session.kind == SessionKind::Nap
                && in_week(session, week_start_date, week_end)
        })
        .count();
    let nap_total_min = sessions
        .iter()
        .filter(|session| {
            session.confirmed
                && session.kind == SessionKind::Nap
                && in_week(session, week_start_date, week_end)
        })
        .map(Session::duration_minutes)
        .sum();

    let mut longest: Option<(NaiveDate, i64)> = None;
    let mut shortest: Option<(NaiveDate, i64)> = None;
    let mut latest_start: Option<(NaiveDate, NaiveDateTime)> = None;
    let mut earliest_end: Option<(NaiveDate, NaiveDateTime)> = None;
    let mut main_starts = Vec::new();

    for day in &days {
        if day.total_min <= 0 {
            continue;
        }
        if longest
            .map(|(_, value)| day.main_min > value)
            .unwrap_or(true)
        {
            longest = Some((day.date.expect("day always has a date"), day.main_min));
        }
        if shortest
            .map(|(_, value)| day.main_min < value)
            .unwrap_or(true)
        {
            shortest = Some((day.date.expect("day always has a date"), day.main_min));
        }
        if let Some(start) = day.main_start {
            if latest_start.map(|(_, value)| start > value).unwrap_or(true) {
                latest_start = Some((day.date.expect("day always has a date"), start));
            }
            main_starts.push(start);
        }
        if let Some(end) = day.main_end {
            if earliest_end.map(|(_, value)| end < value).unwrap_or(true) {
                earliest_end = Some((day.date.expect("day always has a date"), end));
            }
        }
    }

    let regularity_sd_min = circular_sd_minutes(&main_starts);
    let previous_total_min = {
        let previous_week_start = week_start_date - Duration::days(7);
        let previous_week_end = week_start_date;
        let total: i64 = previous_sessions
            .iter()
            .filter(|session| session.confirmed && session.kind.counts_as_sleep())
            .filter(|session| in_week(session, previous_week_start, previous_week_end))
            .map(Session::duration_minutes)
            .sum();
        if total > 0 {
            Some(total)
        } else {
            None
        }
    };

    let mut facts = Vec::new();
    if session_count == 0 {
        facts.push(Fact {
            kind: FactKind::Data,
            text: "За эту неделю пока нет подтверждённых сессий сна.".to_string(),
        });
    } else {
        facts.push(Fact {
            kind: FactKind::Summary,
            text: format!(
                "Средний сон за дни с данными — {}.",
                format_duration(avg_per_day_min)
            ),
        });
        if let Some(previous) = previous_total_min {
            facts.push(Fact {
                kind: FactKind::Trend,
                text: format!(
                    "За неделю сон изменился на {} относительно прошлой.",
                    format_signed_duration(total_min - previous)
                ),
            });
        }
        if debt_min > 0 {
            facts.push(Fact {
                kind: FactKind::Debt,
                text: format!(
                    "Недосып относительно цели — {} за {} дн. с данными.",
                    format_duration(debt_min),
                    days_with_data
                ),
            });
        } else if debt_min < 0 {
            facts.push(Fact {
                kind: FactKind::Debt,
                text: format!("Сон выше цели на {}.", format_duration(-debt_min)),
            });
        }
        if let Some((date, minutes)) = longest {
            facts.push(Fact {
                kind: FactKind::Longest,
                text: format!(
                    "Самый длинный основной сон — {}, {}.",
                    weekday_ru(date),
                    format_duration(minutes)
                ),
            });
        }
        if let Some((date, minutes)) = shortest {
            if longest.map(|(_, value)| value) != Some(minutes) {
                facts.push(Fact {
                    kind: FactKind::Shortest,
                    text: format!(
                        "Самый короткий основной сон — {}, {}.",
                        weekday_ru(date),
                        format_duration(minutes)
                    ),
                });
            }
        }
        if let Some(sd) = regularity_sd_min {
            facts.push(Fact {
                kind: FactKind::Regularity,
                text: format!(
                    "Время отбоя различалось примерно на ±{}.",
                    format_duration(sd)
                ),
            });
        }
        if let Some((date, start)) = latest_start {
            facts.push(Fact {
                kind: FactKind::Time,
                text: format!(
                    "Самый поздний отбой — {}, {}.",
                    weekday_ru(date),
                    format_time_local(start)
                ),
            });
        }
        if nap_count > 0 {
            facts.push(Fact {
                kind: FactKind::Nap,
                text: format!(
                    "Дневных снов: {}, суммарно {}.",
                    nap_count,
                    format_duration(nap_total_min)
                ),
            });
        }
        if days_with_data < 7 {
            facts.push(Fact {
                kind: FactKind::Data,
                text: format!("Данные есть за {} из 7 дней.", days_with_data),
            });
        }
        let short_days = days
            .iter()
            .filter(|day| day.total_min > 0 && day.total_min < 6 * 60)
            .count();
        if short_days > 0 {
            facts.push(Fact {
                kind: FactKind::Shortest,
                text: format!("Дней сном короче 6 часов: {}.", short_days),
            });
        }
    }

    WeekReport {
        week_start: week_start_date,
        week_end,
        days,
        total_min,
        avg_per_day_min,
        days_with_data,
        session_count,
        nap_count,
        nap_total_min,
        debt_min,
        longest,
        shortest,
        latest_start,
        earliest_end,
        regularity_sd_min,
        previous_total_min,
        facts,
    }
}

fn in_week(session: &Session, start: NaiveDate, end: NaiveDate) -> bool {
    let date = local_datetime(session.end_utc, session.end_offset_min).date();
    date >= start && date < end
}

fn circular_sd_minutes(starts: &[NaiveDateTime]) -> Option<i64> {
    if starts.len() < 2 {
        return None;
    }
    let mut sin_sum = 0.0;
    let mut cos_sum = 0.0;
    for start in starts {
        let angle = minutes_of_day(*start) / 1440.0 * std::f64::consts::TAU;
        sin_sum += angle.sin();
        cos_sum += angle.cos();
    }
    let count = starts.len() as f64;
    let r = (sin_sum.hypot(cos_sum) / count).clamp(0.000_001, 1.0);
    let sd_rad = (-2.0 * r.ln()).sqrt();
    let minutes = (sd_rad / std::f64::consts::TAU * 1440.0).round() as i64;
    Some(minutes.min(12 * 60))
}

pub fn week_title(week_start_date: NaiveDate) -> String {
    let end = week_start_date + Duration::days(6);
    format!(
        "{} — {}",
        format_date_ru(week_start_date),
        format_date_ru(end)
    )
}

pub fn weekday_short(date: NaiveDate) -> &'static str {
    weekday_ru(date)
}

pub fn month_day(date: NaiveDate) -> String {
    format!("{} {}", date.day(), crate::time::month_ru(date.month()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PowerSource;
    use chrono::{TimeZone, Utc};

    fn session(start: (u32, u32, u32, u32), end: (u32, u32, u32, u32), confirmed: bool) -> Session {
        Session {
            id: 0,
            start_utc: Utc
                .with_ymd_and_hms(2026, start.0, start.1, start.2, start.3, 0)
                .unwrap(),
            end_utc: Utc
                .with_ymd_and_hms(2026, end.0, end.1, end.2, end.3, 0)
                .unwrap(),
            start_offset_min: 0,
            end_offset_min: 0,
            kind: SessionKind::Sleep,
            source: PowerSource::Shutdown,
            confidence: 95,
            confirmed,
            note: String::new(),
            created_utc: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
            updated_utc: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn groups_by_wake_date_and_excludes_pending() {
        let week = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let sessions = vec![
            session((11, 16, 23, 0), (11, 17, 7, 0), true),
            session((11, 18, 2, 0), (11, 18, 5, 0), true),
            session((11, 19, 1, 0), (11, 19, 9, 0), false),
        ];
        let report = build_week_report(&sessions, &[], week, 480);
        assert_eq!(report.total_min, 8 * 60 + 3 * 60);
        assert_eq!(report.days_with_data, 2);
        assert_eq!(
            report.days[1].date,
            Some(NaiveDate::from_ymd_opt(2026, 11, 17).unwrap())
        );
        assert_eq!(report.days[1].total_min, 480);
        assert_eq!(report.days[3].pending, 1);
        assert_eq!(report.debt_min, 960 - 660);
    }

    #[test]
    fn regularity_is_zero_for_same_times() {
        let week = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let sessions = vec![
            session((11, 16, 23, 0), (11, 17, 7, 0), true),
            session((11, 17, 23, 0), (11, 18, 7, 0), true),
        ];
        let report = build_week_report(&sessions, &[], week, 480);
        assert_eq!(report.regularity_sd_min, Some(0));
    }

    #[test]
    fn previous_week_delta() {
        let week = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
        let previous = vec![session((11, 10, 23, 0), (11, 11, 6, 0), true)];
        let current = vec![session((11, 16, 23, 0), (11, 17, 8, 0), true)];
        let report = build_week_report(&current, &previous, week, 480);
        assert_eq!(report.previous_total_min, Some(7 * 60));
        assert_eq!(report.total_min, 9 * 60);
    }
}
