use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use chrono::{Datelike, Duration, Local, NaiveDate, Timelike, Utc};
use circadian_core::model::{
    Gap, PersistedState, Session, SessionKind, Settings, Theme as ThemeSetting,
};
use circadian_core::report::{build_week_report, FactKind, WeekReport};
use circadian_core::time::{
    format_clock, format_date_ru, format_duration, format_signed_duration, local_date,
    local_datetime, local_offset_minutes, week_start, weekday_ru,
};
use circadian_core::Store;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::platform;
use crate::theme::{self, palette, Palette};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Today,
    Journal,
    Week,
    Settings,
}

#[derive(Clone)]
struct DayGroup {
    date: NaiveDate,
    total_min: i64,
    pending: usize,
    sessions: Vec<Session>,
}

pub struct AppView {
    store: Rc<Store>,
    data_dir: PathBuf,
    page: Page,
    settings: Settings,
    state: PersistedState,
    sessions: Vec<Session>,
    gaps: Vec<Gap>,
    week_start: NaiveDate,
    notice: Option<String>,
    confirm_delete: Option<i64>,
}

impl AppView {
    pub fn new(store: Rc<Store>, data_dir: PathBuf, page: Page) -> Self {
        let today = Local::now().date_naive();
        Self {
            store,
            data_dir,
            page,
            settings: Settings::default(),
            state: PersistedState::default(),
            sessions: Vec::new(),
            gaps: Vec::new(),
            week_start: week_start(today),
            notice: None,
            confirm_delete: None,
        }
    }

    fn load(mut self) -> Self {
        self.reload();
        self
    }

    fn reload(&mut self) {
        match self.store.load_settings() {
            Ok(settings) => self.settings = settings,
            Err(error) => self.notice = Some(format!("Настройки: {error}")),
        }
        match self.store.load_state() {
            Ok(state) => self.state = state,
            Err(error) => self.notice = Some(format!("Состояние: {error}")),
        }
        match self.store.all_sessions() {
            Ok(sessions) => self.sessions = sessions,
            Err(error) => self.notice = Some(format!("Сессии: {error}")),
        }
        let now = Utc::now();
        match self.store.gaps_between(now - Duration::days(30), now) {
            Ok(gaps) => self.gaps = gaps,
            Err(error) => self.notice = Some(format!("Пробелы: {error}")),
        }
    }

    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.reload();
            cx.notify();
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.reload();
        cx.notify();
    }

    fn confirm_session(&mut self, id: i64, kind: SessionKind, cx: &mut Context<Self>) {
        if let Err(error) = self.store.set_kind(id, kind, true) {
            self.notice = Some(format!("Не удалось сохранить: {error}"));
        }
        self.reload();
        cx.notify();
    }

    fn request_delete(&mut self, id: i64, cx: &mut Context<Self>) {
        self.confirm_delete = Some(id);
        cx.notify();
    }

    fn delete_session(&mut self, id: i64, cx: &mut Context<Self>) {
        if let Err(error) = self.store.delete_session(id) {
            self.notice = Some(format!("Не удалось удалить: {error}"));
        }
        self.confirm_delete = None;
        self.reload();
        cx.notify();
    }

    fn commit_settings(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.store.save_settings(&self.settings) {
            self.notice = Some(format!("Настройки: {error}"));
        }
        platform::set_autostart(self.settings.autostart);
        theme::apply(cx, self.settings.theme);
        cx.notify();
    }

    fn pending(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.iter().filter(|s| !s.confirmed).collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.end_utc));
        sessions
    }

    fn last_session(&self) -> Option<&Session> {
        let mut main = self
            .sessions
            .iter()
            .filter(|s| s.confirmed && s.kind.counts_as_sleep() && s.duration_minutes() >= 180)
            .max_by_key(|s| s.end_utc);
        if main.is_none() {
            main = self
                .sessions
                .iter()
                .filter(|s| s.confirmed && s.kind.counts_as_sleep())
                .max_by_key(|s| s.end_utc);
        }
        main
    }

    fn today_sessions(&self, date: NaiveDate) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| local_date(s.end_utc, s.end_offset_min) == date)
            .collect();
        sessions.sort_by_key(|s| s.start_utc);
        sessions
    }

    fn seven_day_report(&self, today: NaiveDate) -> WeekReport {
        build_week_report(
            &self.sessions,
            &self.sessions,
            today - Duration::days(6),
            self.settings.target_sleep_minutes,
        )
    }

    fn week_report(&self) -> WeekReport {
        build_week_report(
            &self.sessions,
            &self.sessions,
            self.week_start,
            self.settings.target_sleep_minutes,
        )
    }

    fn day_groups(&self, today: NaiveDate) -> Vec<DayGroup> {
        let cutoff = today - Duration::days(365);
        let mut map: BTreeMap<NaiveDate, Vec<Session>> = BTreeMap::new();
        for session in &self.sessions {
            let date = local_date(session.end_utc, session.end_offset_min);
            if date < cutoff {
                continue;
            }
            map.entry(date).or_default().push(session.clone());
        }
        map.into_iter()
            .rev()
            .map(|(date, mut sessions)| {
                sessions.sort_by_key(|s| s.start_utc);
                let total_min = sessions
                    .iter()
                    .filter(|s| s.confirmed && s.kind.counts_as_sleep())
                    .map(Session::duration_minutes)
                    .sum();
                let pending = sessions.iter().filter(|s| !s.confirmed).count();
                DayGroup {
                    date,
                    total_min,
                    pending,
                    sessions,
                }
            })
            .collect()
    }

    fn status(&self, p: Palette) -> (String, String, Hsla) {
        let now = Utc::now();
        if self
            .state
            .paused_until_utc
            .map(|until| until > now)
            .unwrap_or(false)
        {
            let until = self.state.paused_until_utc.expect("checked");
            return (
                "Пауза".to_string(),
                format!(
                    "до {}",
                    local_datetime(until, local_offset_minutes(until)).format("%H:%M")
                ),
                p.amber,
            );
        }
        if let Some(start) = self.state.manual_sleep_start_utc {
            return (
                "Сон вручную".to_string(),
                format!("с {}", format_clock(start, local_offset_minutes(start))),
                p.accent,
            );
        }
        let awake = self
            .state
            .awake_since_utc
            .map(|start| (now - start).num_minutes().max(0))
            .unwrap_or(0);
        ("Бодрствуешь".to_string(), format_duration(awake), p.green)
    }

    fn page_title(&self) -> (&'static str, String) {
        let today = Local::now().date_naive();
        match self.page {
            Page::Today => (
                "Сегодня",
                format!(
                    "{} · цель {}",
                    format_date_ru(today),
                    format_duration(self.settings.target_sleep_minutes)
                ),
            ),
            Page::Journal => ("Журнал", "Все сессии по дню пробуждения".to_string()),
            Page::Week => (
                "Неделя",
                circadian_core::report::week_title(self.week_start),
            ),
            Page::Settings => ("Настройки", "Пороги, тема и хранение данных".to_string()),
        }
    }

    fn sidebar(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let (status, detail, status_color) = self.status(p);
        let last = self.last_session().map(|session| {
            format!(
                "{} · {}",
                format_duration(session.duration_minutes()),
                format_clock(session.end_utc, session.end_offset_min)
            )
        });

        div()
            .flex()
            .flex_col()
            .w(px(248.))
            .h_full()
            .p_5()
            .gap_6()
            .bg(linear_gradient(
                180.,
                linear_color_stop(p.surface, 0.),
                linear_color_stop(p.bg, 1.),
            ))
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(44.))
                            .rounded(px(14.))
                            .bg(p.accent.opacity(0.16))
                            .child(Icon::new(IconName::Moon).size(px(23.)).text_color(p.accent)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(19.))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Circadian"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child("сон без ручного ввода"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(self.nav_item(p, "nav-today", IconName::Sun, "Сегодня", Page::Today, cx))
                    .child(self.nav_item(
                        p,
                        "nav-journal",
                        IconName::Calendar,
                        "Журнал",
                        Page::Journal,
                        cx,
                    ))
                    .child(self.nav_item(
                        p,
                        "nav-week",
                        IconName::ChartPie,
                        "Неделя",
                        Page::Week,
                        cx,
                    ))
                    .child(self.nav_item(
                        p,
                        "nav-settings",
                        IconName::Settings,
                        "Настройки",
                        Page::Settings,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .rounded(px(18.))
                    .bg(p.surface2)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(8.)).rounded(px(99.)).bg(status_color))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(status),
                            )
                            .child(div().flex_1())
                            .child(div().text_size(px(12.)).text_color(p.muted).child(detail)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(p.muted)
                                    .child("ПОСЛЕДНИЙ СОН"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child(last.unwrap_or_else(|| "пока нет данных".to_string())),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child("v0.1 · локальные данные"),
                    )
                    .child(
                        Button::new("sidebar-refresh")
                            .ghost()
                            .compact()
                            .icon(IconName::RefreshCw)
                            .tooltip("Обновить")
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            )
    }

    fn nav_item(
        &self,
        p: Palette,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        page: Page,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.page == page;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .rounded(px(12.))
            .cursor_pointer()
            .bg(if active {
                p.accent.opacity(0.13)
            } else {
                p.surface.opacity(0.0)
            })
            .text_color(if active { p.accent } else { p.muted })
            .when(active, |this| {
                this.border_1().border_color(p.accent.opacity(0.18))
            })
            .child(Icon::new(icon).size(px(18.)).text_color(if active {
                p.accent
            } else {
                p.muted
            }))
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(if active {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .child(label),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_page(page, cx)))
    }

    fn topbar(&self, p: Palette) -> Div {
        let (title, subtitle) = self.page_title();
        let pending = self.pending().len();
        let today = Local::now().date_naive();
        div()
            .flex()
            .items_center()
            .justify_between()
            .px_8()
            .py_5()
            .border_b_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(24.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(div().text_size(px(12.)).text_color(p.muted).child(subtitle)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(pending > 0, |this| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .px_3()
                                .py_1()
                                .rounded(px(99.))
                                .bg(p.amber.opacity(0.12))
                                .text_color(p.amber)
                                .child(Icon::new(IconName::TriangleAlert).size(px(14.)))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(attention_text(pending)),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1()
                            .rounded(px(99.))
                            .bg(p.surface2)
                            .border_1()
                            .border_color(p.border)
                            .child(
                                Icon::new(IconName::Calendar)
                                    .size(px(14.))
                                    .text_color(p.muted),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.muted)
                                    .child(format_date_ru(today)),
                            ),
                    ),
            )
    }

    fn content(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.page {
            Page::Today => self.today_page(p, cx),
            Page::Journal => self.journal_page(p, cx),
            Page::Week => self.week_page(p, cx),
            Page::Settings => self.settings_page(p, cx),
        };
        let mut container = div()
            .id("page-scroll")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .child(body);
        if let Some(notice) = &self.notice {
            container = container.child(
                div().px_8().pb_6().child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .p_3()
                        .rounded(px(12.))
                        .bg(p.rose.opacity(0.10))
                        .border_1()
                        .border_color(p.rose.opacity(0.3))
                        .text_color(p.rose)
                        .child(Icon::new(IconName::TriangleAlert).size(px(16.)))
                        .child(div().text_size(px(12.)).child(notice.clone()))
                        .child(div().flex_1())
                        .child(
                            Button::new("dismiss-notice")
                                .ghost()
                                .compact()
                                .icon(IconName::Close)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notice = None;
                                    cx.notify();
                                })),
                        ),
                ),
            );
        }
        container
    }

    fn today_page(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let today = Local::now().date_naive();
        let pending = self.pending();
        let sessions = self.today_sessions(today);
        let report = self.seven_day_report(today);
        let last = self.last_session();
        let (status, detail, status_color) = self.status(p);

        let mut page = div().flex().flex_col().gap_6().p_8();

        if !pending.is_empty() {
            page = page.child(self.pending_card(p, &pending, cx));
        }

        let hero = div()
            .flex()
            .gap_8()
            .p_6()
            .rounded(px(22.))
            .bg(linear_gradient(
                135.,
                linear_color_stop(p.surface, 0.),
                linear_color_stop(p.surface2, 1.),
            ))
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .flex_1()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child("ПОСЛЕДНИЙ СОН"),
                    )
                    .child(
                        div()
                            .text_size(px(40.))
                            .font_weight(FontWeight::BOLD)
                            .child(
                                last.map(|session| format_duration(session.duration_minutes()))
                                    .unwrap_or_else(|| "—".to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(13.))
                            .text_color(p.muted)
                            .child(
                                last.map(|session| {
                                    format!(
                                        "{} – {}",
                                        format_clock(session.start_utc, session.start_offset_min),
                                        format_clock(session.end_utc, session.end_offset_min)
                                    )
                                })
                                .unwrap_or_else(|| "Данных пока нет".to_string()),
                            )
                            .when_some(last, |this, session| {
                                this.child(chip(p, session.source.label_ru(), p.muted))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .w(px(250.))
                    .p_4()
                    .rounded(px(16.))
                    .bg(p.bg.opacity(0.55))
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(9.)).rounded(px(99.)).bg(status_color))
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(status),
                            ),
                    )
                    .child(div().text_size(px(13.)).text_color(p.muted).child(detail))
                    .child(div().h(px(1.)).w_full().bg(p.border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(11.)).text_color(p.muted).child("Цель"))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child(format_duration(self.settings.target_sleep_minutes)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_size(px(11.)).text_color(p.muted).child("7 дней"))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child(if report.days_with_data > 0 {
                                        format_duration(report.avg_per_day_min)
                                    } else {
                                        "—".to_string()
                                    }),
                            ),
                    ),
            );

        page = page.child(hero);

        let today_total: i64 = sessions
            .iter()
            .filter(|s| s.confirmed && s.kind.counts_as_sleep())
            .map(|s| s.duration_minutes())
            .sum();
        let debt_label = if report.debt_min > 0 {
            format_duration(report.debt_min)
        } else if report.debt_min < 0 {
            format!("запас {}", format_duration(-report.debt_min))
        } else {
            "—".to_string()
        };
        let regularity = report
            .regularity_sd_min
            .map(|minutes| format!("±{}", format_duration(minutes)))
            .unwrap_or_else(|| "—".to_string());

        page = page.child(
            div()
                .flex()
                .gap_4()
                .child(self.stat_card(
                    p,
                    "Сон сегодня",
                    format_duration(today_total),
                    if today_total == 0 {
                        "ещё нет сессий".to_string()
                    } else {
                        format!("{} сессий", sessions.iter().filter(|s| s.confirmed).count())
                    },
                    p.accent,
                ))
                .child(self.stat_card(
                    p,
                    "Среднее за 7 дней",
                    if report.days_with_data > 0 {
                        format_duration(report.avg_per_day_min)
                    } else {
                        "—".to_string()
                    },
                    format!("{} дн. с данными", report.days_with_data),
                    p.cyan,
                ))
                .child(self.stat_card(
                    p,
                    if report.debt_min >= 0 {
                        "Недосып"
                    } else {
                        "Запас"
                    },
                    debt_label,
                    format!(
                        "цель {}",
                        format_duration(self.settings.target_sleep_minutes)
                    ),
                    if report.debt_min > 0 {
                        p.amber
                    } else {
                        p.green
                    },
                ))
                .child(self.stat_card(
                    p,
                    "Разброс отбоя",
                    regularity,
                    "по последним 7 дням".to_string(),
                    p.accent,
                )),
        );

        page = page.child(self.timeline_card(p, today, &sessions));

        if let Some(card) = self.recent_sessions_card(p, cx) {
            page = page.child(card);
        }

        if !self.gaps.is_empty() {
            page = page.child(self.gaps_card(p));
        }

        if sessions.is_empty() && last.is_none() {
            page = page.child(self.empty_state(
                p,
                "Пока нет данных",
                "Демон начнёт отслеживание после первого выключения ПК. Держи Circadian в трее — дальше всё само.",
                IconName::Moon,
            ));
        }

        page
    }

    fn stat_card(
        &self,
        p: Palette,
        label: &'static str,
        value: String,
        hint: String,
        color: Hsla,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .flex_1()
            .min_w(px(150.))
            .p_4()
            .rounded(px(18.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().size(px(6.)).rounded(px(99.)).bg(color))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(p.muted)
                            .child(label.to_uppercase()),
                    ),
            )
            .child(
                div()
                    .text_size(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(value),
            )
            .child(div().text_size(px(11.)).text_color(p.muted).child(hint))
    }

    fn pending_card(&self, p: Palette, pending: &[&Session], cx: &mut Context<Self>) -> Div {
        let mut card = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .rounded(px(20.))
            .bg(p.amber.opacity(0.07))
            .border_1()
            .border_color(p.amber.opacity(0.28))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(34.))
                            .rounded(px(11.))
                            .bg(p.amber.opacity(0.15))
                            .child(
                                Icon::new(IconName::TriangleAlert)
                                    .size(px(18.))
                                    .text_color(p.amber),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Нужно подтвердить"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child("Это сон, короткий уход или перезагрузка?"),
                            ),
                    )
                    .child(div().flex_1()),
            );

        for session in pending.iter().take(3) {
            let id = session.id;
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(12.))
                    .bg(p.bg.opacity(0.45))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_size(px(11.)).text_color(p.muted).child(
                                format_date_ru(local_date(session.end_utc, session.end_offset_min)),
                            ))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_family("JetBrains Mono")
                                    .child(session_range(session)),
                            )
                            .child(div().text_size(px(11.)).text_color(p.muted).child(format!(
                                "{} · {}",
                                format_duration(session.duration_minutes()),
                                session.source.label_ru()
                            ))),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new(format!("pending-sleep-{id}"))
                            .success()
                            .compact()
                            .flex_shrink_0()
                            .label("Это сон")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirm_session(id, SessionKind::Sleep, cx)
                            })),
                    )
                    .child(
                        Button::new(format!("pending-away-{id}"))
                            .ghost()
                            .compact()
                            .flex_shrink_0()
                            .label("Не сон")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirm_session(id, SessionKind::Away, cx)
                            })),
                    ),
            );
        }
        if pending.len() > 3 {
            card = card.child(
                div()
                    .text_size(px(11.))
                    .text_color(p.muted)
                    .child(format!("и ещё {}", pending.len() - 3)),
            );
        }
        card
    }

    fn timeline_card(&self, p: Palette, date: NaiveDate, sessions: &[&Session]) -> Div {
        let now = Utc::now();
        let now_fraction = if local_date(now, local_offset_minutes(now)) == date {
            let local = local_datetime(now, local_offset_minutes(now));
            (local.time().num_seconds_from_midnight() as f32 / 86_400.0).clamp(0., 1.)
        } else {
            -1.0
        };

        let mut track = div()
            .relative()
            .h(px(58.))
            .w_full()
            .rounded(px(14.))
            .overflow_hidden()
            .bg(p.surface2)
            .border_1()
            .border_color(p.border);

        for index in 0..8 {
            track = track.child(
                div()
                    .absolute()
                    .top(px(0.))
                    .bottom(px(0.))
                    .left(DefiniteLength::Fraction(index as f32 / 8.0))
                    .w(px(1.))
                    .bg(p.border.opacity(0.45)),
            );
        }

        for session in sessions {
            let start = local_datetime(session.start_utc, session.start_offset_min);
            let end = local_datetime(session.end_utc, session.end_offset_min);
            if end.date() < date || start.date() > date {
                continue;
            }
            let start_sec = if start.date() < date {
                0.0
            } else {
                start.time().num_seconds_from_midnight() as f32
            };
            let end_sec = if end.date() > date {
                86_400.0
            } else {
                end.time().num_seconds_from_midnight() as f32
            };
            let start_frac = (start_sec / 86_400.0).clamp(0., 1.);
            let end_frac = (end_sec / 86_400.0).clamp(0., 1.);
            if end_frac <= start_frac {
                continue;
            }
            let color = session_color(p, session);
            track = track.child(
                div()
                    .absolute()
                    .top(px(11.))
                    .h(px(36.))
                    .left(DefiniteLength::Fraction(start_frac))
                    .w(DefiniteLength::Fraction((end_frac - start_frac).max(0.003)))
                    .rounded(px(9.))
                    .bg(color.opacity(if session.confirmed { 0.9 } else { 0.55 }))
                    .border_1()
                    .border_color(color.opacity(0.55)),
            );
        }

        if now_fraction >= 0.0 {
            track = track.child(
                div()
                    .absolute()
                    .top(px(0.))
                    .bottom(px(0.))
                    .left(DefiniteLength::Fraction(now_fraction))
                    .w(px(2.))
                    .bg(p.text.opacity(0.85)),
            );
        }

        let mut labels = div().flex().w_full().pt_2();
        for label in ["00", "03", "06", "09", "12", "15", "18", "21", "24"] {
            labels = labels.child(
                div()
                    .flex_1()
                    .text_size(px(9.))
                    .text_color(p.muted)
                    .child(label),
            );
        }

        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .rounded(px(20.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Таймлайн дня"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(format_date_ru(date)),
                    ),
            )
            .child(track)
            .child(labels)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(legend(p, p.accent, "сон"))
                    .child(legend(p, p.amber, "проверить"))
                    .child(legend(p, p.muted, "не сон")),
            )
    }

    fn delete_action(&self, id: i64, cx: &mut Context<Self>) -> Div {
        if self.confirm_delete == Some(id) {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(
                    Button::new(format!("journal-delete-confirm-{id}"))
                        .danger()
                        .compact()
                        .label("Удалить")
                        .on_click(cx.listener(move |this, _, _, cx| this.delete_session(id, cx))),
                )
                .child(
                    Button::new(format!("journal-delete-cancel-{id}"))
                        .ghost()
                        .compact()
                        .icon(IconName::Close)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirm_delete = None;
                            cx.notify();
                        })),
                )
        } else {
            div().child(
                Button::new(format!("journal-delete-{id}"))
                    .ghost()
                    .compact()
                    .icon(IconName::Delete)
                    .tooltip("Удалить сессию")
                    .on_click(cx.listener(move |this, _, _, cx| this.request_delete(id, cx))),
            )
        }
    }

    fn recent_sessions_card(&self, p: Palette, cx: &mut Context<Self>) -> Option<Div> {
        let mut sessions: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.confirmed && s.kind.counts_as_sleep())
            .collect();
        sessions.sort_by_key(|s| std::cmp::Reverse(s.end_utc));
        sessions.truncate(5);
        if sessions.is_empty() {
            return None;
        }

        let mut card = div()
            .flex()
            .flex_col()
            .rounded(px(20.))
            .overflow_hidden()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_5()
                    .py_4()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Последние сессии"),
                    )
                    .child(
                        Button::new("recent-open-journal")
                            .ghost()
                            .compact()
                            .label("Весь журнал")
                            .icon(IconName::ArrowRight)
                            .on_click(
                                cx.listener(|this, _, _, cx| this.set_page(Page::Journal, cx)),
                            ),
                    ),
            );

        for session in sessions {
            let date = local_date(session.end_utc, session.end_offset_min);
            let color = session_color(p, session);
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_5()
                    .py_3()
                    .border_t_1()
                    .border_color(p.border.opacity(0.65))
                    .child(div().w(px(3.)).h(px(24.)).rounded(px(9.)).bg(color))
                    .child(
                        div()
                            .w(px(112.))
                            .text_size(px(12.))
                            .child(format_date_ru(date)),
                    )
                    .child(
                        div()
                            .w(px(150.))
                            .text_size(px(12.))
                            .font_family("JetBrains Mono")
                            .child(session_range(session)),
                    )
                    .child(
                        div()
                            .w(px(88.))
                            .text_size(px(12.))
                            .child(format_duration(session.duration_minutes())),
                    )
                    .child(chip(p, session.kind.label_ru(), color))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(session.source.label_ru()),
                    ),
            );
        }

        Some(card)
    }

    fn gaps_card(&self, p: Palette) -> Div {
        let mut card = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_5()
            .rounded(px(20.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::Calendar)
                            .size(px(16.))
                            .text_color(p.amber),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Пробелы в данных"),
                    ),
            );
        for gap in self.gaps.iter().take(4) {
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .child(div().font_family("JetBrains Mono").child(format!(
                        "{} – {}",
                        gap.start_utc.with_timezone(&Local).format("%d.%m %H:%M"),
                        gap.end_utc.with_timezone(&Local).format("%H:%M")
                    )))
                    .child(div().child(gap.reason.clone())),
            );
        }
        card
    }

    fn journal_page(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let today = Local::now().date_naive();
        let groups = self.day_groups(today);
        let session_count: usize = groups.iter().map(|group| group.sessions.len()).sum();
        let mut page = div().flex().flex_col().gap_5().p_8();
        page = page.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{} сессий", session_count)),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(p.muted)
                                .child("показаны последние 365 дней · дата = день пробуждения"),
                        ),
                )
                .child(
                    Button::new("journal-refresh")
                        .secondary()
                        .compact()
                        .icon(IconName::RefreshCw)
                        .label("Обновить")
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
        );

        if groups.is_empty() {
            return page.child(self.empty_state(
                p,
                "Журнал пуст",
                "Как только демон зафиксирует первый сон, здесь появится таблица по дням.",
                IconName::Calendar,
            ));
        }

        for group in groups {
            page = page.child(self.day_group(p, &group, cx));
        }
        page
    }

    fn day_group(&self, p: Palette, group: &DayGroup, cx: &mut Context<Self>) -> Div {
        let mut card = div()
            .flex()
            .flex_col()
            .rounded(px(18.))
            .overflow_hidden()
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .py_3()
                    .bg(p.surface2)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format_date_ru(group.date)),
                            )
                            .when(group.pending > 0, |this| {
                                this.child(chip(
                                    p,
                                    &format!("{} проверить", group.pending),
                                    p.amber,
                                ))
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(if group.total_min > 0 {
                                format_duration(group.total_min)
                            } else {
                                "—".to_string()
                            }),
                    ),
            );

        card = card.child(
            div()
                .flex()
                .items_center()
                .px_4()
                .py_2()
                .gap_3()
                .text_size(px(9.))
                .text_color(p.muted)
                .child(div().w(px(168.)).child("ВРЕМЯ"))
                .child(div().w(px(92.)).child("СОН"))
                .child(div().w(px(110.)).child("ТИП"))
                .child(div().w(px(118.)).child("ИСТОЧНИК"))
                .child(div().flex_1().child("СТАТУС"))
                .child(div().w(px(34.))),
        );

        for session in &group.sessions {
            let id = session.id;
            let color = session_color(p, session);
            let status = if !session.confirmed {
                "проверить".to_string()
            } else if session.kind == SessionKind::Away {
                "не сон".to_string()
            } else {
                "подтверждён".to_string()
            };
            let row =
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(p.border.opacity(0.65))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .w(px(168.))
                            .child(div().w(px(3.)).h(px(22.)).rounded(px(9.)).bg(color))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_family("JetBrains Mono")
                                    .child(session_range(session)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(92.))
                            .text_size(px(12.))
                            .child(format_duration(session.duration_minutes())),
                    )
                    .child(div().w(px(110.)).flex().items_center().child(chip(
                        p,
                        session.kind.label_ru(),
                        color,
                    )))
                    .child(
                        div()
                            .w(px(118.))
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child(session.source.label_ru()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_1()
                            .when(!session.confirmed, |this| {
                                this.child(chip(p, &status, p.amber))
                            })
                            .when(session.confirmed, |this| {
                                this.child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_size(px(11.))
                                        .text_color(p.muted)
                                        .child(
                                            Icon::new(if session.kind == SessionKind::Away {
                                                IconName::Close
                                            } else {
                                                IconName::CircleCheck
                                            })
                                            .size(px(13.))
                                            .text_color(if session.kind == SessionKind::Away {
                                                p.muted
                                            } else {
                                                p.green
                                            }),
                                        )
                                        .child(status),
                                )
                            })
                            .when(
                                session.confirmed && session.kind == SessionKind::Away,
                                |this| {
                                    this.child(
                                        Button::new(format!("journal-restore-{id}"))
                                            .ghost()
                                            .compact()
                                            .label("Это сон")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.confirm_session(id, SessionKind::Sleep, cx)
                                            })),
                                    )
                                },
                            )
                            .when(!session.confirmed, |this| {
                                this.child(
                                    Button::new(format!("journal-sleep-{id}"))
                                        .success()
                                        .compact()
                                        .label("Сон")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.confirm_session(id, SessionKind::Sleep, cx)
                                        })),
                                )
                                .child(
                                    Button::new(format!("journal-away-{id}"))
                                        .ghost()
                                        .compact()
                                        .label("Не сон")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.confirm_session(id, SessionKind::Away, cx)
                                        })),
                                )
                            }),
                    )
                    .child(self.delete_action(id, cx));
            card = card.child(row);
        }

        card
    }

    fn week_page(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let report = self.week_report();
        let current_week = week_start(Local::now().date_naive());
        let max_minutes = report
            .days
            .iter()
            .map(|day| day.total_min)
            .max()
            .unwrap_or(0)
            .max(self.settings.target_sleep_minutes)
            .max(60) as f32
            * 1.15;
        let chart_height = 150.0;

        let mut page = div().flex().flex_col().gap_5().p_8();
        page = page.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::new("week-prev")
                                .secondary()
                                .compact()
                                .icon(IconName::ChevronLeft)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.week_start -= Duration::days(7);
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(circadian_core::report::week_title(self.week_start)),
                        )
                        .child(
                            Button::new("week-next")
                                .secondary()
                                .compact()
                                .icon(IconName::ChevronRight)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.week_start += Duration::days(7);
                                    cx.notify();
                                })),
                        )
                        .when(self.week_start != current_week, |this| {
                            this.child(
                                Button::new("week-now")
                                    .ghost()
                                    .compact()
                                    .label("К текущей")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.week_start = current_week;
                                        cx.notify();
                                    })),
                            )
                        }),
                )
                .child(div().text_size(px(11.)).text_color(p.muted).child(format!(
                    "цель {}",
                    format_duration(self.settings.target_sleep_minutes)
                ))),
        );

        if report.session_count == 0 {
            return page.child(self.empty_state(
                p,
                "За эту неделю нет данных",
                "Переключись на другую неделю или подожди, пока демон соберёт первые сессии.",
                IconName::ChartPie,
            ));
        }

        let debt_value = if report.debt_min > 0 {
            format_duration(report.debt_min)
        } else if report.debt_min < 0 {
            format!("запас {}", format_duration(-report.debt_min))
        } else {
            "—".to_string()
        };
        let delta_hint = report
            .previous_total_min
            .map(|prev| {
                format!(
                    "{} к прошлой",
                    format_signed_duration(report.total_min - prev)
                )
            })
            .unwrap_or_else(|| "нет прошлой недели".to_string());
        let regularity = report
            .regularity_sd_min
            .map(|minutes| format!("±{}", format_duration(minutes)))
            .unwrap_or_else(|| "—".to_string());

        page = page.child(
            div()
                .flex()
                .gap_4()
                .child(self.stat_card(
                    p,
                    "Всего сна",
                    format_duration(report.total_min),
                    format!("{} сессий", report.session_count),
                    p.accent,
                ))
                .child(self.stat_card(
                    p,
                    "Среднее",
                    format_duration(report.avg_per_day_min),
                    format!("{} дн. с данными", report.days_with_data),
                    p.cyan,
                ))
                .child(self.stat_card(
                    p,
                    if report.debt_min >= 0 {
                        "Недосып"
                    } else {
                        "Запас"
                    },
                    debt_value,
                    delta_hint,
                    if report.debt_min > 0 {
                        p.amber
                    } else {
                        p.green
                    },
                ))
                .child(self.stat_card(
                    p,
                    "Регулярность",
                    regularity,
                    "разброс отбоя".to_string(),
                    p.accent,
                )),
        );

        let mut bars = div().flex().items_end().gap_3().h(px(200.));
        for day in &report.days {
            let main_h = if day.main_min > 0 {
                (day.main_min as f32 / max_minutes * chart_height).max(5.0)
            } else {
                0.0
            };
            let nap_h = if day.nap_min > 0 {
                (day.nap_min as f32 / max_minutes * chart_height).max(4.0)
            } else {
                0.0
            };
            let total = day.total_min;
            let label = day
                .date
                .map(|date| format!("{} {}", weekday_ru(date), date.day()))
                .unwrap_or_default();
            let target_y = self.settings.target_sleep_minutes as f32 / max_minutes * chart_height;
            bars = bars.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(if total > 0 { p.text } else { p.muted })
                            .child(if total > 0 {
                                format_duration(total)
                            } else {
                                String::new()
                            }),
                    )
                    .child(
                        div()
                            .relative()
                            .w(px(30.))
                            .h(px(chart_height))
                            .flex()
                            .flex_col()
                            .justify_end()
                            .rounded(px(9.))
                            .overflow_hidden()
                            .when(day.nap_min > 0, |this| {
                                this.child(div().h(px(nap_h)).w_full().bg(p.cyan))
                            })
                            .when(day.main_min > 0, |this| {
                                this.child(div().h(px(main_h)).w_full().bg(p.accent))
                            })
                            .when(total == 0, |this| {
                                this.child(div().h(px(3.)).w_full().bg(p.border))
                            })
                            .child(
                                div()
                                    .absolute()
                                    .left(px(0.))
                                    .bottom(px(target_y))
                                    .w_full()
                                    .h(px(1.))
                                    .bg(p.amber.opacity(0.55)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(if day.date == Some(Local::now().date_naive()) {
                                p.accent
                            } else {
                                p.muted
                            })
                            .child(label),
                    ),
            );
        }

        page = page.child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .p_5()
                .rounded(px(20.))
                .bg(p.surface)
                .border_1()
                .border_color(p.border)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Сон по дням"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_4()
                                .child(legend(p, p.accent, "сон"))
                                .child(legend(p, p.cyan, "дневной"))
                                .child(legend(p, p.amber, "цель")),
                        ),
                )
                .child(bars),
        );

        let mut facts = div()
            .flex()
            .flex_col()
            .rounded(px(20.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_5()
                    .py_4()
                    .border_b_1()
                    .border_color(p.border)
                    .child(Icon::new(IconName::Star).size(px(16.)).text_color(p.accent))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Факты недели"),
                    ),
            );
        for fact in &report.facts {
            let color = match fact.kind {
                FactKind::Debt | FactKind::Shortest => p.amber,
                FactKind::Longest | FactKind::Summary => p.accent,
                FactKind::Regularity | FactKind::Time => p.cyan,
                FactKind::Trend => p.green,
                FactKind::Nap => p.cyan,
                FactKind::Data => p.muted,
            };
            facts = facts.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_5()
                    .py_3()
                    .border_b_1()
                    .border_color(p.border.opacity(0.55))
                    .child(div().size(px(6.)).rounded(px(99.)).bg(color))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text)
                            .child(fact.text.clone()),
                    ),
            );
        }
        page = page.child(facts);

        page
    }

    fn settings_page(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let mut page = div().flex().flex_col().gap_5().p_8();

        page = page.child(
            self.settings_card(
                p,
                "Цель и пороги",
                "Как Circadian отличает сон от ребута и короткого ухода.",
            )
            .child(self.stepper_row(
                p,
                "goal",
                "Цель сна",
                format_duration(self.settings.target_sleep_minutes),
                "используется в недельном отчёте",
                cx,
                |this, cx| {
                    this.settings.target_sleep_minutes =
                        (this.settings.target_sleep_minutes - 30).max(4 * 60);
                    this.commit_settings(cx);
                },
                |this, cx| {
                    this.settings.target_sleep_minutes =
                        (this.settings.target_sleep_minutes + 30).min(12 * 60);
                    this.commit_settings(cx);
                },
            ))
            .child(self.stepper_row(
                p,
                "ignore",
                "Игнорировать до",
                format!("{} мин", self.settings.ignore_below_minutes),
                "ребуты и мгновенные перерывы",
                cx,
                |this, cx| {
                    this.settings.ignore_below_minutes =
                        (this.settings.ignore_below_minutes - 5).max(5);
                    this.commit_settings(cx);
                },
                |this, cx| {
                    this.settings.ignore_below_minutes =
                        (this.settings.ignore_below_minutes + 5).min(60);
                    this.commit_settings(cx);
                },
            ))
            .child(self.stepper_row(
                p,
                "pending",
                "Спрашивать до",
                format_duration(self.settings.pending_below_minutes),
                "короче — не сон, длиннее — уже сон",
                cx,
                |this, cx| {
                    this.settings.pending_below_minutes =
                        (this.settings.pending_below_minutes - 15).max(30);
                    this.commit_settings(cx);
                },
                |this, cx| {
                    this.settings.pending_below_minutes = (this.settings.pending_below_minutes
                        + 15)
                        .min(this.settings.auto_confirm_minutes - 15);
                    this.commit_settings(cx);
                },
            ))
            .child(self.stepper_row(
                p,
                "confirm",
                "Автосон от",
                format_duration(self.settings.auto_confirm_minutes),
                "такие перерывы считаются сном сразу",
                cx,
                |this, cx| {
                    this.settings.auto_confirm_minutes =
                        (this.settings.auto_confirm_minutes - 30).max(120);
                    this.commit_settings(cx);
                },
                |this, cx| {
                    this.settings.auto_confirm_minutes =
                        (this.settings.auto_confirm_minutes + 30).min(12 * 60);
                    this.commit_settings(cx);
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pt_2()
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(div().text_size(px(13.)).child("Считать сон ПК сном"))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(p.muted)
                                    .child("suspend и гибернация как обычный сон"),
                            ),
                    )
                    .child(
                        Switch::new("suspend-switch")
                            .checked(self.settings.suspend_counts_as_sleep)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.settings.suspend_counts_as_sleep = *value;
                                this.commit_settings(cx);
                            })),
                    ),
            ),
        );

        page = page.child(
            self.settings_card(p, "Приложение", "Автозапуск, тема и поведение окна.")
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .child("Автозапуск при входе в Windows"),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(p.muted)
                                        .child("демон в трее запускается автоматически"),
                                ),
                        )
                        .child(
                            Switch::new("autostart-switch")
                                .checked(self.settings.autostart)
                                .on_change(cx.listener(|this, value, _, cx| {
                                    this.settings.autostart = *value;
                                    this.commit_settings(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .pt_2()
                        .border_t_1()
                        .border_color(p.border)
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_size(px(13.)).child("Тема"))
                                .child(
                                    div()
                                        .text_size(px(11.))
                                        .text_color(p.muted)
                                        .child("Circadian dark / light"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(self.theme_pill(
                                    p,
                                    "theme-dark",
                                    "Тёмная",
                                    ThemeSetting::Dark,
                                    cx,
                                ))
                                .child(self.theme_pill(
                                    p,
                                    "theme-light",
                                    "Светлая",
                                    ThemeSetting::Light,
                                    cx,
                                ))
                                .child(self.theme_pill(
                                    p,
                                    "theme-system",
                                    "Система",
                                    ThemeSetting::System,
                                    cx,
                                )),
                        ),
                ),
        );

        page = page.child(
            self.settings_card(
                p,
                "Данные",
                "Всё хранится локально. Никакой сети и телеметрии.",
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.muted)
                            .child("ПАПКА ДАННЫХ"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_family("JetBrains Mono")
                            .text_color(p.muted)
                            .child(self.data_dir.display().to_string()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        Button::new("open-folder")
                            .secondary()
                            .compact()
                            .icon(IconName::FolderOpen)
                            .label("Открыть папку")
                            .on_click(|_, _, _| platform::open_folder()),
                    )
                    .child(
                        Button::new("export-csv")
                            .secondary()
                            .compact()
                            .icon(IconName::FileText)
                            .label("Экспорт CSV")
                            .on_click(cx.listener(|this, _, _, cx| this.export_csv(cx))),
                    )
                    .child(
                        Button::new("settings-refresh")
                            .ghost()
                            .compact()
                            .icon(IconName::RefreshCw)
                            .label("Обновить")
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    ),
            ),
        );

        page = page.child(
            self.settings_card(p, "О программе", "Circadian")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_size(px(11.))
                        .text_color(p.muted)
                        .child("Нативный Rust + GPUI, без WebView. Данные — SQLite в LocalAppData.")
                        .child("GPUI и GPUI Kit — Apache-2.0. Шрифты Inter и JetBrains Mono — OFL.")
                        .child("Трекер: выключение ПК = сон, включение = пробуждение. Спорные перерывы требуют одного клика."),
                ),
        );

        page
    }

    fn settings_card(&self, p: Palette, title: &'static str, subtitle: &'static str) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .p_5()
            .rounded(px(20.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(div().text_size(px(11.)).text_color(p.muted).child(subtitle)),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn stepper_row(
        &self,
        p: Palette,
        id: &'static str,
        label: &'static str,
        value: String,
        hint: &'static str,
        cx: &mut Context<Self>,
        minus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        plus: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .pt_2()
            .border_t_1()
            .border_color(p.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_size(px(13.)).child(label))
                    .child(div().text_size(px(11.)).text_color(p.muted).child(hint)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .p_1()
                    .rounded(px(12.))
                    .bg(p.surface2)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        Button::new(format!("{id}-minus"))
                            .ghost()
                            .compact()
                            .icon(IconName::Minus)
                            .on_click(cx.listener(move |this, _, _, cx| minus(this, cx))),
                    )
                    .child(
                        div()
                            .w(px(94.))
                            .flex()
                            .justify_center()
                            .text_center()
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(value),
                    )
                    .child(
                        Button::new(format!("{id}-plus"))
                            .ghost()
                            .compact()
                            .icon(IconName::Plus)
                            .on_click(cx.listener(move |this, _, _, cx| plus(this, cx))),
                    ),
            )
    }

    fn theme_pill(
        &self,
        p: Palette,
        id: &'static str,
        label: &'static str,
        value: ThemeSetting,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.settings.theme == value;
        div()
            .id(id)
            .flex()
            .items_center()
            .justify_center()
            .px_3()
            .py_1()
            .rounded(px(99.))
            .cursor_pointer()
            .bg(if active { p.primary } else { p.surface2 })
            .text_color(if active {
                rgb(0xffffff).into()
            } else {
                p.muted
            })
            .border_1()
            .border_color(if active { p.primary } else { p.border })
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings.theme = value;
                this.commit_settings(cx);
            }))
    }

    fn export_csv(&mut self, cx: &mut Context<Self>) {
        let path = self.data_dir.join(format!(
            "circadian-{}.csv",
            Local::now().format("%Y%m%d-%H%M")
        ));
        let mut csv = String::from(
            "start_utc,end_utc,start_local,end_local,minutes,kind,source,confirmed,note\n",
        );
        for session in &self.sessions {
            let start = local_datetime(session.start_utc, session.start_offset_min);
            let end = local_datetime(session.end_utc, session.end_offset_min);
            let note = session.note.replace('"', "\"\"");
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},\"{}\"\n",
                session.start_utc.to_rfc3339(),
                session.end_utc.to_rfc3339(),
                start.format("%Y-%m-%d %H:%M"),
                end.format("%Y-%m-%d %H:%M"),
                session.duration_minutes(),
                session.kind.as_str(),
                session.source.as_str(),
                session.confirmed as i32,
                note
            ));
        }
        match std::fs::write(&path, csv) {
            Ok(()) => {
                self.notice = Some(format!("CSV сохранён: {}", path.display()));
                let _ = std::process::Command::new("explorer")
                    .arg("/select,")
                    .arg(path)
                    .spawn();
            }
            Err(error) => self.notice = Some(format!("Не удалось сохранить CSV: {error}")),
        }
        cx.notify();
    }

    fn empty_state(
        &self,
        p: Palette,
        title: &'static str,
        text: &'static str,
        icon: IconName,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .py_16()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(64.))
                    .rounded(px(22.))
                    .bg(p.accent.opacity(0.12))
                    .child(Icon::new(icon).size(px(30.)).text_color(p.accent)),
            )
            .child(
                div()
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(420.))
                    .text_size(px(12.))
                    .text_color(p.muted)
                    .text_center()
                    .child(text),
            )
    }
}

impl Render for AppView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .id("app-root")
            .flex()
            .size_full()
            .overflow_hidden()
            .bg(p.bg)
            .text_color(p.text)
            .child(
                div().flex().size_full().child(self.sidebar(p, cx)).child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .h_full()
                        .min_w_0()
                        .child(self.topbar(p))
                        .child(self.content(p, cx)),
                ),
            )
    }
}

pub fn make(store: Rc<Store>, data_dir: PathBuf, page: Page) -> AppView {
    AppView::new(store, data_dir, page).load()
}

fn attention_text(count: usize) -> String {
    if count == 1 {
        "1 ждёт подтверждения".to_string()
    } else {
        format!("{count} ждут подтверждения")
    }
}

fn chip(p: Palette, text: &str, color: Hsla) -> Div {
    let _ = p;
    div()
        .px_2()
        .py_1()
        .rounded(px(99.))
        .bg(color.opacity(0.13))
        .text_color(color)
        .text_size(px(10.))
        .font_weight(FontWeight::MEDIUM)
        .child(text.to_string())
}

fn legend(p: Palette, color: Hsla, text: &'static str) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(div().size(px(6.)).rounded(px(99.)).bg(color))
        .child(div().text_size(px(10.)).text_color(p.muted).child(text))
}

fn session_color(p: Palette, session: &Session) -> Hsla {
    if !session.confirmed {
        return p.amber;
    }
    match session.kind {
        SessionKind::Sleep => p.accent,
        SessionKind::Nap => p.cyan,
        SessionKind::Away => p.muted,
        SessionKind::Unknown => p.muted,
    }
}

fn session_range(session: &Session) -> String {
    let start = local_datetime(session.start_utc, session.start_offset_min);
    let end = local_datetime(session.end_utc, session.end_offset_min);
    let span = end.date().signed_duration_since(start.date()).num_days();
    let suffix = if span > 0 {
        format!(" +{span}д")
    } else {
        String::new()
    };
    format!(
        "{} – {}{}",
        start.format("%H:%M"),
        end.format("%H:%M"),
        suffix
    )
}
