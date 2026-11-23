use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    System,
    English,
    Russian,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    English,
    Russian,
}

impl Lang {
    pub fn resolve(setting: Language, system: Lang) -> Self {
        match setting {
            Language::English => Self::English,
            Language::Russian => Self::Russian,
            Language::System => system,
        }
    }

    pub fn is_russian(self) -> bool {
        self == Self::Russian
    }

    pub fn t(self, ru: &'static str) -> &'static str {
        if self.is_russian() {
            ru
        } else {
            translate(ru)
        }
    }
}

pub fn translate(ru: &'static str) -> &'static str {
    match ru {
        "Состояние" => "State",
        "Сессии" => "Sessions",
        "Пробелы" => "Gaps",
        "Не удалось сохранить" => "Could not save",
        "Не удалось удалить" => "Could not delete",
        "CSV сохранён" => "CSV saved",
        "Не удалось сохранить CSV" => "Could not save CSV",
        "до" => "until",
        "с" => "since",
        "запас" => "surplus",
        "нет прошлой недели" => "no previous week",
        "локальные данные" => "local data",
        "ждут подтверждения" => "need review",
        "Язык" => "Language",
        "Язык интерфейса и отчётов" => "Interface and report language",
        "Релизы" => "Releases",
        "Русский" => "Russian",
        "Русский интерфейс" => "Russian interface",
        "Переключение между английским и русским" => "Switch between English and Russian",
        "Сообщить о проблеме" => "Report an issue",
        "Трекер сна для Windows. Без браслета, телефона и облака." => "A sleep tracker for Windows. No wearable, phone or cloud.",
        "Все данные остаются на этом компьютере." => "All data stays on this computer.",
        "Не удалось запустить демон" => "Could not start the daemon",
        "Не удалось открыть базу данных" => "Could not open the database",
        "Не удалось получить модуль" => "Could not get the module",
        "Не удалось создать окно демона" => "Could not create the daemon window",
        "Сегодня" => "Today",
        "Журнал" => "Journal",
        "Неделя" => "Week",
        "Настройки" => "Settings",
        "сон без ручного ввода" => "automatic sleep tracking",
        "Все сессии по дню пробуждения" => "All sessions by wake date",
        "Пороги, тема и хранение данных" => "Thresholds, theme and data",
        "ПОСЛЕДНИЙ СОН" => "LAST SLEEP",
        "пока нет данных" => "no data yet",
        "Данных пока нет" => "No data yet",
        "Цель" => "Goal",
        "7 дней" => "7 days",
        "Бодрствуешь" => "Awake",
        "Пауза" => "Paused",
        "Сон вручную" => "Manual sleep",
        "Сон сегодня" => "Sleep today",
        "Среднее за 7 дней" => "7-day average",
        "Недосып" => "Sleep debt",
        "Запас" => "Surplus",
        "Разброс отбоя" => "Bedtime spread",
        "ещё нет сессий" => "no sessions yet",
        "0 дн. с данными" => "0 days with data",
        "цель" => "goal",
        "по последним 7 дням" => "last 7 days",
        "Пока нет данных" => "No data yet",
        "Демон начнёт отслеживание после первого выключения ПК. Держи Circadian в трее — дальше всё само." => "The daemon starts tracking after your first PC shutdown. Keep Circadian in the tray and it will do the rest.",
        "Нужно подтвердить" => "Needs review",
        "Это сон, короткий уход или перезагрузка?" => "Was this sleep, a short break, or a reboot?",
        "Это сон" => "It was sleep",
        "Не сон" => "Not sleep",
        "Сон" => "Sleep",
        "Дневной сон" => "Nap",
        "Неизвестно" => "Unknown",
        "Выключение ПК" => "PC shutdown",
        "Сон ПК" => "PC sleep",
        "Гибернация" => "Hibernation",
        "Включение ПК" => "PC startup",
        "Вручную" => "Manual",
        "Таймлайн дня" => "Day timeline",
        "сон" => "sleep",
        "проверить" => "review",
        "не сон" => "not sleep",
        "Удалить" => "Delete",
        "Удалить сессию" => "Delete session",
        "Последние сессии" => "Recent sessions",
        "Весь журнал" => "Full journal",
        "Пробелы в данных" => "Data gaps",
        "сессий" => "sessions",
        "показаны последние 365 дней · дата = день пробуждения" => "last 365 days · date = wake date",
        "Обновить" => "Refresh",
        "Журнал пуст" => "Journal is empty",
        "Как только демон зафиксирует первый сон, здесь появится таблица по дням." => "Once the daemon records your first sleep, the daily table will appear here.",
        "подтверждён" => "confirmed",
        "ВРЕМЯ" => "TIME",
        "СОН" => "SLEEP",
        "ТИП" => "TYPE",
        "ИСТОЧНИК" => "SOURCE",
        "СТАТУС" => "STATUS",
        "К текущей" => "This week",
        "За эту неделю нет данных" => "No data for this week",
        "Переключись на другую неделю или подожди, пока демон соберёт первые сессии." => "Switch to another week or wait until the daemon collects the first sessions.",
        "Всего сна" => "Total sleep",
        "Среднее" => "Average",
        "Регулярность" => "Regularity",
        "разброс отбоя" => "bedtime spread",
        "Сон по дням" => "Sleep by day",
        "дневной" => "daytime",
        "Факты недели" => "Weekly facts",
        "Цель и пороги" => "Goals and thresholds",
        "Как Circadian отличает сон от ребута и короткого ухода." => "How Circadian tells sleep from a reboot or a short break.",
        "Цель сна" => "Sleep goal",
        "используется в недельном отчёте" => "used in the weekly report",
        "Игнорировать до" => "Ignore below",
        "ребуты и мгновенные перерывы" => "reboots and quick breaks",
        "Спрашивать от" => "Review gaps from",
        "от этого порога до автосна — подтверждение" => "gaps from here until auto-sleep need review",
        "демон не работал" => "daemon was not running",
        "Автосон от" => "Auto-sleep from",
        "такие перерывы считаются сном сразу" => "these breaks count as sleep immediately",
        "Считать сон ПК сном" => "Count PC sleep as sleep",
        "suspend и гибернация как обычный сон" => "suspend and hibernate count as regular sleep",
        "Приложение" => "App",
        "Автозапуск, тема и поведение окна." => "Autostart, theme and window behavior.",
        "Автозапуск при входе в Windows" => "Launch at Windows sign-in",
        "демон в трее запускается автоматически" => "the tray daemon starts automatically",
        "Тема" => "Theme",
        "Тёмная" => "Dark",
        "Светлая" => "Light",
        "Система" => "System",
        "Данные" => "Data",
        "Всё хранится локально. Никакой сети и телеметрии." => "Everything stays local. No network calls, no telemetry.",
        "ПАПКА ДАННЫХ" => "DATA FOLDER",
        "Открыть папку" => "Open folder",
        "Экспорт CSV" => "Export CSV",
        "О программе" => "About",
        "Версия" => "Version",
        "Настройки: {error}" => "Settings: {error}",
        "Состояние: {error}" => "State: {error}",
        "Сессии: {error}" => "Sessions: {error}",
        "Пробелы: {error}" => "Gaps: {error}",
        "Не удалось сохранить: {error}" => "Could not save: {error}",
        "Не удалось удалить: {error}" => "Could not delete: {error}",
        "CSV сохранён: {path}" => "CSV saved: {path}",
        "Не удалось сохранить CSV: {error}" => "Could not save CSV: {error}",
        "1 ждёт подтверждения" => "Needs review",
        "и ещё" => "and",
        "Минута" => "minute",
        "мин" => "min",
        "ч" => "h",
        "д" => "d",
        "Сон вручную: идёт" => "Manual sleep: active",
        "Проверить обновления" => "Check for updates",
        "Доступно обновление" => "Update available",
        "Установлена последняя версия" => "You're up to date",
        "Не удалось проверить обновления" => "Could not check for updates",
        "Портативная версия обновляется вручную." => "The portable build updates manually.",
        "Позже" => "Later",
        "Установка…" => "Installing…",
        "Скачивание…" => "Downloading…",
        "Обновить и перезапустить" => "Update and restart",
        "Скачать ZIP" => "Download ZIP",
        "Размер" => "Size",
        "Portable: скачайте ZIP и распакуйте вручную." => "Portable: download the ZIP and extract it manually.",
        "Что нового" => "What's new",
        "Устанавливаем обновление…" => "Installing update…",
        "Открыть окно" => "Open window",
        "Открыть Circadian" => "Open Circadian",
        "Я проснулся" => "I'm awake",
        "Я ложусь спать" => "I'm going to sleep",
        "Снять паузу" => "Resume tracking",
        "Пауза 4 часа" => "Pause for 4 hours",
        "Автозапуск" => "Autostart",
        "Настройки…" => "Settings…",
        "Выход" => "Exit",
        "Circadian · пауза" => "Circadian · paused",
        "Circadian · сон вручную с {}" => "Circadian · manual sleep since {}",
        "Circadian · бодрствуешь {}" => "Circadian · awake {}",
        "Не найден circadian.exe рядом с демоном." => "circadian.exe was not found next to the daemon.",
        _ => ru,
    }
}

pub fn count_sessions(lang: Lang, count: usize) -> String {
    if lang.is_russian() {
        let word = match count % 10 {
            1 if count % 100 != 11 => "сессия",
            2..=4 if !(12..=14).contains(&(count % 100)) => "сессии",
            _ => "сессий",
        };
        format!("{count} {word}")
    } else if count == 1 {
        "1 session".to_string()
    } else {
        format!("{count} sessions")
    }
}

pub fn count_days(lang: Lang, count: usize) -> String {
    if lang.is_russian() {
        format!("{count} дн. с данными")
    } else if count == 1 {
        "1 day with data".to_string()
    } else {
        format!("{count} days with data")
    }
}
