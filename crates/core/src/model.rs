use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::i18n::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Sleep,
    Nap,
    Away,
    Unknown,
}

impl SessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sleep => "sleep",
            Self::Nap => "nap",
            Self::Away => "away",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "sleep" => Self::Sleep,
            "nap" => Self::Nap,
            "away" => Self::Away,
            _ => Self::Unknown,
        }
    }

    pub fn counts_as_sleep(self) -> bool {
        matches!(self, Self::Sleep | Self::Nap)
    }

    pub fn label_ru(self) -> &'static str {
        match self {
            Self::Sleep => "Сон",
            Self::Nap => "Дневной сон",
            Self::Away => "Не сон",
            Self::Unknown => "Неизвестно",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSource {
    Shutdown,
    Suspend,
    Hibernate,
    Boot,
    Manual,
}

impl PowerSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shutdown => "shutdown",
            Self::Suspend => "suspend",
            Self::Hibernate => "hibernate",
            Self::Boot => "boot",
            Self::Manual => "manual",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "shutdown" => Self::Shutdown,
            "suspend" => Self::Suspend,
            "hibernate" => Self::Hibernate,
            "boot" => Self::Boot,
            "manual" => Self::Manual,
            _ => Self::Boot,
        }
    }

    pub fn label_ru(self) -> &'static str {
        match self {
            Self::Shutdown => "Выключение ПК",
            Self::Suspend => "Сон ПК",
            Self::Hibernate => "Гибернация",
            Self::Boot => "Включение ПК",
            Self::Manual => "Вручную",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: i64,
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    pub start_offset_min: i32,
    pub end_offset_min: i32,
    pub kind: SessionKind,
    pub source: PowerSource,
    pub confidence: u8,
    pub confirmed: bool,
    pub note: String,
    pub created_utc: DateTime<Utc>,
    pub updated_utc: DateTime<Utc>,
}

impl Session {
    pub fn duration_minutes(&self) -> i64 {
        (self.end_utc - self.start_utc).num_minutes().max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDraft {
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    pub start_offset_min: i32,
    pub end_offset_min: i32,
    pub kind: SessionKind,
    pub source: PowerSource,
    pub confidence: u8,
    pub confirmed: bool,
    pub note: String,
}

impl SessionDraft {
    pub fn duration_minutes(&self) -> i64 {
        (self.end_utc - self.start_utc).num_minutes().max(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GapDraft {
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub id: i64,
    pub start_utc: DateTime<Utc>,
    pub end_utc: DateTime<Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPower {
    pub source: PowerSource,
    pub at_utc: DateTime<Utc>,
    #[serde(default)]
    pub start_hint_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub last_seen_utc: Option<DateTime<Utc>>,
    pub last_boot_hint_utc: Option<DateTime<Utc>>,
    pub pending_power: Option<PendingPower>,
    pub awake_since_utc: Option<DateTime<Utc>>,
    pub manual_sleep_start_utc: Option<DateTime<Utc>>,
    pub paused_until_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub target_sleep_minutes: i64,
    pub ignore_below_minutes: i64,
    pub pending_below_minutes: i64,
    pub auto_confirm_minutes: i64,
    pub suspend_counts_as_sleep: bool,
    pub autostart: bool,
    pub theme: Theme,
    #[serde(default)]
    pub language: Language,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            target_sleep_minutes: 8 * 60,
            ignore_below_minutes: 15,
            pending_below_minutes: 45,
            auto_confirm_minutes: 4 * 60,
            suspend_counts_as_sleep: true,
            autostart: true,
            theme: Theme::Dark,
            language: Language::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    System,
}
