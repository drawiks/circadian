use std::fs;
use std::mem::{size_of, zeroed};
use std::path::PathBuf;
use std::process::Command;

use chrono::{DateTime, Utc};
use circadian_core::i18n::Lang;
use circadian_core::reconcile::{reconcile_start, StartContext};
use circadian_core::time::{format_clock, format_duration, local_offset_minutes, now_utc};
use circadian_core::{PersistedState, PowerSource, SessionDraft, Settings, Store};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowLongPtrW, LoadIconW, LoadImageW,
    MessageBoxW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetForegroundWindow,
    SetTimer, SetWindowLongPtrW, TrackPopupMenu, TranslateMessage, GWLP_USERDATA, IDI_APPLICATION,
    IMAGE_ICON, LR_LOADFROMFILE, MB_ICONERROR, MB_OK, MF_CHECKED, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_DESTROY, WM_ENDSESSION, WM_POWERBROADCAST,
    WM_QUERYENDSESSION, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

const TIMER_HEARTBEAT: usize = 1;
const HEARTBEAT_MS: u32 = 5 * 60 * 1000;
const TRAY_CALLBACK: u32 = WM_APP + 1;
const ID_OPEN: usize = 1001;
const ID_OPEN_SETTINGS: usize = 1002;
const ID_TOGGLE_SLEEP: usize = 1003;
const ID_PAUSE: usize = 1004;
const ID_AUTOSTART: usize = 1005;
const ID_EXIT: usize = 1006;
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Circadian";
const ENDSESSION_LOGOFF: u32 = 0x8000_0000;

pub fn run() {
    let mutex_name = wide("Local\\CircadianDaemon");
    let _mutex = match unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) } {
        Ok(handle) => {
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                return;
            }
            handle
        }
        Err(error) => {
            show_error(&format!("Не удалось запустить демон: {error}"));
            return;
        }
    };

    let mut app = match App::new() {
        Ok(app) => app,
        Err(error) => {
            show_error(&format!("Не удалось открыть базу данных: {error}"));
            return;
        }
    };

    let instance = unsafe { GetModuleHandleW(None) }.map(|module| HINSTANCE(module.0));
    let hinstance = match instance {
        Ok(instance) => instance,
        Err(error) => {
            show_error(&format!("Не удалось получить модуль: {error}"));
            return;
        }
    };

    let class_name = wide("CircadianDaemonWindow");
    let window_name = wide("Circadian Daemon");
    let wc = WNDCLASSW {
        style: Default::default(),
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: app.tray_icon,
        hCursor: Default::default(),
        hbrBackground: Default::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: PCWSTR(class_name.as_ptr()),
    };
    unsafe { RegisterClassW(&wc) };

    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_name.as_ptr()),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        )
    };
    let hwnd = match hwnd {
        Ok(hwnd) => hwnd,
        Err(error) => {
            show_error(&format!("Не удалось создать окно демона: {error}"));
            return;
        }
    };

    app.hwnd = hwnd;
    app.taskbar_created =
        unsafe { RegisterWindowMessageW(PCWSTR(wide("TaskbarCreated").as_ptr())) };
    let app_ptr = Box::into_raw(Box::new(app));
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, app_ptr as isize) };

    unsafe {
        if let Some(app) = app_ptr.as_mut() {
            app.add_tray();
            app.update_tooltip();
        }
        SetTimer(Some(hwnd), TIMER_HEARTBEAT, HEARTBEAT_MS, None);
    }

    let mut message: MSG = unsafe { zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    unsafe {
        let mut app = Box::from_raw(app_ptr);
        app.remove_tray();
        if !app.tray_icon.is_invalid() {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyIcon(app.tray_icon);
        }
    }
}

struct App {
    store: Store,
    settings: Settings,
    state: PersistedState,
    lang: Lang,
    system_lang: Lang,
    hwnd: HWND,
    tray_icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    tray_added: bool,
    taskbar_created: u32,
    data_dir: PathBuf,
}

impl App {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = data_dir();
        fs::create_dir_all(&data_dir)?;
        let store = Store::open(data_dir.join("circadian.db"))?;
        let settings = store.load_settings()?;
        let mut state = store.load_state()?;
        let now = now_utc();
        let boot = boot_hint();
        let outcome = reconcile_start(
            &state,
            &settings,
            StartContext {
                now,
                boot_hint_utc: boot,
            },
        );

        if let Some(session) = &outcome.session {
            store.try_insert_session(session)?;
        }
        if let Some(gap) = &outcome.gap {
            store.insert_gap_if_new(gap)?;
        }
        store.save_state(&outcome.state)?;
        state = outcome.state;
        if state.awake_since_utc.is_none() {
            state.awake_since_utc = Some(now);
            store.save_state(&state)?;
        }
        store.insert_event(now, "daemon_start", "start")?;
        apply_autostart(settings.autostart);

        let system_lang = system_language();
        let lang = Lang::resolve(settings.language, system_lang);
        let tray_icon = load_icon();
        let app = Self {
            store,
            settings,
            state,
            lang,
            system_lang,
            hwnd: HWND::default(),
            tray_icon,
            tray_added: false,
            taskbar_created: 0,
            data_dir,
        };
        app.log("start", "daemon started");
        Ok(app)
    }

    fn t(&self, ru: &'static str) -> &'static str {
        self.lang.t(ru)
    }

    fn reload_settings(&mut self) {
        if let Ok(settings) = self.store.load_settings() {
            self.lang = Lang::resolve(settings.language, self.system_lang);
            self.settings = settings;
        }
    }

    fn log(&self, kind: &str, detail: &str) {
        let logs = self.data_dir.join("logs");
        if fs::create_dir_all(&logs).is_err() {
            return;
        }
        let path = logs.join("daemon.log");
        if fs::metadata(&path)
            .map(|meta| meta.len() > 1_000_000)
            .unwrap_or(false)
        {
            let old = logs.join("daemon.old.log");
            let _ = fs::remove_file(&old);
            let _ = fs::rename(&path, old);
        }
        let line = format!(
            "{} {kind} {detail}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S")
        );
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
    }

    fn tick(&mut self) {
        self.reload_settings();
        if self.settings.autostart {
            apply_autostart(true);
        }
        let now = now_utc();
        self.state.last_seen_utc = Some(now);
        if self
            .state
            .paused_until_utc
            .map(|until| until <= now)
            .unwrap_or(false)
        {
            self.state.paused_until_utc = None;
        }
        if let Err(error) = self.store.save_state(&self.state) {
            self.log("error", &format!("heartbeat: {error}"));
        }
        self.update_tooltip();
    }

    fn on_query_end(&mut self) {
        let now = now_utc();
        if self.is_paused(now) {
            self.state.last_seen_utc = Some(now);
            let _ = self.store.save_state(&self.state);
            return;
        }
        if self
            .state
            .pending_power
            .as_ref()
            .map(|pending| pending.source == PowerSource::Shutdown)
            .unwrap_or(false)
        {
            self.state.last_seen_utc = Some(now);
            let _ = self.store.save_state(&self.state);
            return;
        }
        let manual = self.state.manual_sleep_start_utc;
        self.state.pending_power = Some(circadian_core::PendingPower {
            source: if manual.is_some() {
                PowerSource::Manual
            } else {
                PowerSource::Shutdown
            },
            at_utc: now,
            start_hint_utc: manual,
        });
        self.state.manual_sleep_start_utc = None;
        self.state.last_seen_utc = Some(now);
        if let Err(error) = self.store.save_state(&self.state) {
            self.log("error", &format!("shutdown state: {error}"));
        }
        let _ = self.store.insert_event(now, "shutdown", "query_end");
        self.log("shutdown", "system is going down");
    }

    fn on_suspend(&mut self) {
        let now = now_utc();
        if self.is_paused(now) {
            self.state.last_seen_utc = Some(now);
            let _ = self.store.save_state(&self.state);
            return;
        }
        let manual = self.state.manual_sleep_start_utc;
        self.state.pending_power = Some(circadian_core::PendingPower {
            source: if manual.is_some() {
                PowerSource::Manual
            } else {
                PowerSource::Suspend
            },
            at_utc: now,
            start_hint_utc: manual,
        });
        self.state.manual_sleep_start_utc = None;
        self.state.last_seen_utc = Some(now);
        if let Err(error) = self.store.save_state(&self.state) {
            self.log("error", &format!("suspend state: {error}"));
        }
        let _ = self.store.insert_event(now, "suspend", "");
        self.log("suspend", "system is suspending");
    }

    fn on_end_session(&mut self) {
        let now = now_utc();
        self.state.last_seen_utc = Some(now);
        let _ = self.store.save_state(&self.state);
    }

    fn on_resume(&mut self) {
        let now = now_utc();
        if self.state.pending_power.is_none() {
            let manual = self.state.manual_sleep_start_utc;
            self.state.pending_power = Some(circadian_core::PendingPower {
                source: if manual.is_some() {
                    PowerSource::Manual
                } else {
                    PowerSource::Suspend
                },
                at_utc: self.state.last_seen_utc.unwrap_or(now),
                start_hint_utc: manual,
            });
        }
        let outcome = reconcile_start(
            &self.state,
            &self.settings,
            StartContext {
                now,
                boot_hint_utc: boot_hint(),
            },
        );
        if let Some(session) = &outcome.session {
            if let Err(error) = self.store.try_insert_session(session) {
                self.log("error", &format!("resume session: {error}"));
            }
        }
        if let Some(gap) = &outcome.gap {
            let _ = self.store.insert_gap_if_new(gap);
        }
        self.state = outcome.state;
        if let Err(error) = self.store.save_state(&self.state) {
            self.log("error", &format!("resume state: {error}"));
        }
        let _ = self.store.insert_event(now, "resume", "");
        self.update_tooltip();
        self.log("resume", "system resumed");
    }

    fn toggle_manual_sleep(&mut self) {
        let now = now_utc();
        if let Some(start) = self.state.manual_sleep_start_utc.take() {
            if now > start {
                let draft = SessionDraft {
                    start_utc: start,
                    end_utc: now,
                    start_offset_min: local_offset_minutes(start),
                    end_offset_min: local_offset_minutes(now),
                    kind: circadian_core::SessionKind::Sleep,
                    source: PowerSource::Manual,
                    confidence: 100,
                    confirmed: true,
                    note: String::new(),
                };
                let _ = self.store.insert_session(&draft);
            }
            self.state.awake_since_utc = Some(now);
            self.state.last_seen_utc = Some(now);
        } else {
            self.state.manual_sleep_start_utc = Some(now);
            self.state.last_seen_utc = Some(now);
        }
        let _ = self.store.save_state(&self.state);
        self.update_tooltip();
    }

    fn toggle_pause(&mut self) {
        let now = now_utc();
        if self
            .state
            .paused_until_utc
            .map(|until| until > now)
            .unwrap_or(false)
        {
            self.state.paused_until_utc = None;
        } else {
            self.state.paused_until_utc = Some(now + chrono::Duration::hours(4));
        }
        let _ = self.store.save_state(&self.state);
        self.update_tooltip();
    }

    fn toggle_autostart(&mut self) {
        self.settings.autostart = !self.settings.autostart;
        let _ = self.store.save_settings(&self.settings);
        apply_autostart(self.settings.autostart);
    }

    fn is_paused(&self, now: DateTime<Utc>) -> bool {
        self.state
            .paused_until_utc
            .map(|until| until > now)
            .unwrap_or(false)
    }

    fn tooltip_text(&self) -> String {
        let now = now_utc();
        if self.is_paused(now) {
            return self.t("Circadian · пауза").to_string();
        }
        if let Some(start) = self.state.manual_sleep_start_utc {
            if self.lang.is_russian() {
                return format!(
                    "Circadian · сон вручную с {}",
                    format_clock(start, local_offset_minutes(start))
                );
            }
            return format!(
                "Circadian · manual sleep since {}",
                format_clock(start, local_offset_minutes(start))
            );
        }
        let awake = self
            .state
            .awake_since_utc
            .map(|start| (now - start).num_minutes().max(0))
            .unwrap_or(0);
        if self.lang.is_russian() {
            format!(
                "Circadian · бодрствуешь {}",
                format_duration(awake, self.lang)
            )
        } else {
            format!("Circadian · awake {}", format_duration(awake, self.lang))
        }
    }

    fn update_tooltip(&self) {
        if !self.tray_added {
            return;
        }
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_TIP;
        data.szTip = fixed_wide::<128>(&self.tooltip_text());
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    fn add_tray(&mut self) {
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY_CALLBACK;
        data.hIcon = self.tray_icon;
        data.szTip = fixed_wide::<128>(&self.tooltip_text());
        let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool();
        self.tray_added = ok;
        if !ok {
            self.log("error", "failed to add tray icon");
        }
    }

    fn remove_tray(&mut self) {
        if !self.tray_added {
            return;
        }
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.hwnd;
        data.uID = 1;
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        self.tray_added = false;
    }

    fn show_menu(&mut self) {
        self.reload_settings();
        let menu = match unsafe { CreatePopupMenu() } {
            Ok(menu) => menu,
            Err(_) => return,
        };
        let status = if self.state.manual_sleep_start_utc.is_some() {
            self.t("Сон вручную: идёт")
        } else {
            self.t("Открыть окно")
        };
        append(menu, MF_STRING | MF_GRAYED, 0, status);
        append(menu, MF_SEPARATOR, 0, "");
        append(menu, MF_STRING, ID_OPEN, self.t("Открыть Circadian"));
        let sleep_label = if self.state.manual_sleep_start_utc.is_some() {
            self.t("Я проснулся")
        } else {
            self.t("Я ложусь спать")
        };
        append(menu, MF_STRING, ID_TOGGLE_SLEEP, sleep_label);
        let pause_label = if self
            .state
            .paused_until_utc
            .map(|until| until > now_utc())
            .unwrap_or(false)
        {
            self.t("Снять паузу")
        } else {
            self.t("Пауза 4 часа")
        };
        append(menu, MF_STRING, ID_PAUSE, pause_label);
        append(menu, MF_SEPARATOR, 0, "");
        let autostart_flags = if self.settings.autostart {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        append(menu, autostart_flags, ID_AUTOSTART, self.t("Автозапуск"));
        append(menu, MF_STRING, ID_OPEN_SETTINGS, self.t("Настройки…"));
        append(menu, MF_SEPARATOR, 0, "");
        append(menu, MF_STRING, ID_EXIT, self.t("Выход"));

        let mut point = POINT::default();
        let _ = unsafe { GetCursorPos(&mut point) };
        unsafe {
            let _ = SetForegroundWindow(self.hwnd);
        }
        let command = unsafe {
            TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
                point.x,
                point.y,
                None,
                self.hwnd,
                None,
            )
        }
        .0 as usize;
        unsafe {
            let _ = DestroyMenu(menu);
        }

        match command {
            ID_OPEN => self.open_gui(false),
            ID_OPEN_SETTINGS => self.open_gui(true),
            ID_TOGGLE_SLEEP => self.toggle_manual_sleep(),
            ID_PAUSE => self.toggle_pause(),
            ID_AUTOSTART => self.toggle_autostart(),
            ID_EXIT => unsafe {
                let _ = DestroyWindow(self.hwnd);
            },
            _ => {}
        }
    }

    fn open_gui(&self, settings: bool) {
        let exe = std::env::current_exe().ok();
        let directory = exe
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from);
        let Some(directory) = directory else {
            return;
        };
        for name in ["circadian.exe", "circadian-app.exe"] {
            let candidate = directory.join(name);
            if candidate.exists() {
                let mut command = Command::new(candidate);
                if settings {
                    command.arg("--settings");
                }
                let _ = command.spawn();
                return;
            }
        }
        show_error(self.t("Не найден circadian.exe рядом с демоном."));
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut App;
    if !ptr.is_null() {
        let app = unsafe { &mut *ptr };
        match message {
            WM_QUERYENDSESSION => {
                let logoff = (lparam.0 as u32 & ENDSESSION_LOGOFF) != 0;
                if !logoff {
                    app.on_query_end();
                } else {
                    app.on_end_session();
                }
                return LRESULT(1);
            }
            WM_ENDSESSION => {
                if wparam.0 != 0 {
                    app.on_end_session();
                }
                return LRESULT(0);
            }
            WM_POWERBROADCAST => {
                match wparam.0 as u32 {
                    PBT_APMSUSPEND => app.on_suspend(),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => app.on_resume(),
                    _ => {}
                }
                return LRESULT(1);
            }
            WM_TIMER => {
                if wparam.0 == TIMER_HEARTBEAT {
                    app.tick();
                }
                return LRESULT(0);
            }
            WM_CLOSE => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                };
                return LRESULT(0);
            }
            WM_DESTROY => {
                app.remove_tray();
                unsafe { PostQuitMessage(0) };
                return LRESULT(0);
            }
            _ => {
                if message == TRAY_CALLBACK {
                    match lparam.0 as u32 {
                        windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONUP
                        | windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK => {
                            app.open_gui(false)
                        }
                        windows::Win32::UI::WindowsAndMessaging::WM_RBUTTONUP
                        | windows::Win32::UI::WindowsAndMessaging::WM_CONTEXTMENU => {
                            app.show_menu()
                        }
                        _ => {}
                    }
                    return LRESULT(0);
                }
                if app.taskbar_created != 0 && message == app.taskbar_created {
                    app.tray_added = false;
                    app.add_tray();
                    return LRESULT(0);
                }
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

fn append(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS,
    id: usize,
    label: &str,
) {
    let text = wide(label);
    let _ = unsafe { AppendMenuW(menu, flags, id, PCWSTR(text.as_ptr())) };
}

fn data_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(value).join("Circadian")
    } else if let Some(value) = std::env::var_os("APPDATA") {
        PathBuf::from(value).join("Circadian")
    } else {
        PathBuf::from("Circadian")
    }
}

fn system_language() -> Lang {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        let id = unsafe { GetUserDefaultUILanguage() };
        if id & 0x3ff == 0x19 {
            Lang::Russian
        } else {
            Lang::English
        }
    }
    #[cfg(not(windows))]
    {
        Lang::English
    }
}

fn boot_hint() -> Option<DateTime<Utc>> {
    let millis = unsafe { GetTickCount64() };
    let now = now_utc();
    Some(now - chrono::Duration::milliseconds(millis.min(i64::MAX as u64) as i64))
}

fn apply_autostart(enabled: bool) {
    let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY) else {
        return;
    };
    let key = hkcu.0;
    if enabled {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let value = format!("\"{}\"", exe.display());
        let _ = key.set_value(RUN_VALUE, &value);
    } else {
        let _ = key.delete_value(RUN_VALUE);
    }
}

fn load_icon() -> windows::Win32::UI::WindowsAndMessaging::HICON {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(directory) = exe.parent() {
            let path = directory.join("assets").join("circadian.ico");
            if path.exists() {
                let text = wide(&path.to_string_lossy());
                if let Ok(handle) = unsafe {
                    LoadImageW(
                        None,
                        PCWSTR(text.as_ptr()),
                        IMAGE_ICON,
                        0,
                        0,
                        LR_LOADFROMFILE,
                    )
                } {
                    return windows::Win32::UI::WindowsAndMessaging::HICON(handle.0);
                }
            }
        }
    }
    unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or(
        windows::Win32::UI::WindowsAndMessaging::HICON(std::ptr::null_mut()),
    )
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fixed_wide<const N: usize>(value: &str) -> [u16; N] {
    let mut buffer = [0u16; N];
    let encoded: Vec<u16> = value.encode_utf16().collect();
    let length = encoded.len().min(N.saturating_sub(1));
    buffer[..length].copy_from_slice(&encoded[..length]);
    buffer
}

fn show_error(message: &str) {
    let text = wide(message);
    let caption = wide("Circadian");
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_ICONERROR | MB_OK,
        );
    }
}
