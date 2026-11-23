#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod demo;
mod platform;
mod theme;
mod view;

use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

use circadian_core::i18n::Lang;
use circadian_core::model::Theme as ThemeSetting;
use circadian_core::Store;
use gpui_kit::*;
use view::Page;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let demo_mode = args.iter().any(|arg| arg == "--demo");
    #[cfg(windows)]
    let _app_mutex = if demo_mode {
        None
    } else {
        match claim_single_instance() {
            Some(handle) => Some(handle),
            None => return,
        }
    };

    application().with_assets(assets::Assets).run(move |cx| {
        init(cx);
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../assets/fonts/Inter.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono.ttf").as_slice()),
            ])
            .expect("failed to load bundled fonts");

        let page = parse_page(&args);
        let system_lang = platform::system_language();
        let lang_override = parse_lang(&args);
        let (store, data_dir, initial_week_start, lang_override) = if demo_mode {
            let lang = lang_override.unwrap_or(Lang::English);
            let theme = parse_theme(&args);
            match demo::create(lang, theme) {
                Ok(demo) => {
                    theme::apply(cx, demo.theme);
                    (demo.store, demo.data_dir, Some(demo.week_start), None)
                }
                Err(error) => {
                    eprintln!("failed to create demo data: {error}");
                    return;
                }
            }
        } else {
            let data_dir = platform::data_dir();
            let store = match Store::open(platform::db_path()) {
                Ok(store) => Rc::new(store),
                Err(error) => {
                    eprintln!("failed to open database: {error}");
                    return;
                }
            };
            let settings = store.load_settings().unwrap_or_default();
            theme::apply(cx, settings.theme);
            platform::start_daemon();
            (store, data_dir, None, lang_override)
        };

        let icon = image::load_from_memory(include_bytes!("../assets/circadian.png"))
            .ok()
            .map(|image| Arc::new(image.to_rgba8()));

        let window = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds(
                point(px(120.), px(80.)),
                size(px(1320.), px(860.)),
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Circadian".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_min_size: Some(size(px(1000.), px(680.))),
            icon,
            ..Default::default()
        };

        open_window(window, cx, move |_, cx| {
            cx.new(|_| {
                view::make(
                    store.clone(),
                    data_dir.clone(),
                    page,
                    system_lang,
                    lang_override,
                    initial_week_start,
                )
            })
        })
        .expect("failed to open Circadian window");
    });
}

#[cfg(windows)]
fn claim_single_instance() -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    let name: Vec<u16> = "Local\\CircadianApp\0".encode_utf16().collect();
    let class: Vec<u16> = "Zed::Window\0".encode_utf16().collect();
    let title: Vec<u16> = "Circadian\0".encode_utf16().collect();
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }.ok()?;
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        for _ in 0..20 {
            if let Ok(window) =
                unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(title.as_ptr())) }
            {
                unsafe {
                    let _ = ShowWindow(window, SW_RESTORE);
                    let _ = SetForegroundWindow(window);
                }
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return None;
    }
    Some(handle)
}

fn parse_page(args: &[String]) -> Page {
    if args.iter().any(|arg| arg == "--settings") {
        return Page::Settings;
    }
    for (index, arg) in args.iter().enumerate() {
        if let Some(value) = arg.strip_prefix("--page=") {
            return page_from(value);
        }
        if arg == "--page" {
            if let Some(value) = args.get(index + 1) {
                return page_from(value);
            }
        }
    }
    Page::Today
}

fn page_from(value: &str) -> Page {
    match value {
        "journal" => Page::Journal,
        "week" => Page::Week,
        "settings" => Page::Settings,
        _ => Page::Today,
    }
}

fn parse_lang(args: &[String]) -> Option<Lang> {
    for (index, arg) in args.iter().enumerate() {
        let value = if let Some(value) = arg.strip_prefix("--lang=") {
            Some(value)
        } else if arg == "--lang" {
            args.get(index + 1).map(String::as_str)
        } else {
            None
        };
        if let Some(value) = value {
            return match value {
                "ru" | "russian" => Some(Lang::Russian),
                "en" | "english" => Some(Lang::English),
                _ => None,
            };
        }
    }
    None
}

fn parse_theme(args: &[String]) -> ThemeSetting {
    for (index, arg) in args.iter().enumerate() {
        let value = if let Some(value) = arg.strip_prefix("--theme=") {
            Some(value)
        } else if arg == "--theme" {
            args.get(index + 1).map(String::as_str)
        } else {
            None
        };
        if let Some(value) = value {
            return match value {
                "light" => ThemeSetting::Light,
                "dark" => ThemeSetting::Dark,
                _ => ThemeSetting::System,
            };
        }
    }
    ThemeSetting::Dark
}
