#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod platform;
mod theme;
mod view;

use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

use circadian_core::Store;
use gpui_kit::*;
use view::Page;

fn main() {
    #[cfg(windows)]
    let _app_mutex = match claim_single_instance() {
        Some(handle) => handle,
        None => return,
    };

    application().with_assets(assets::Assets).run(|cx| {
        init(cx);
        cx.text_system()
            .add_fonts(vec![
                Cow::Borrowed(include_bytes!("../assets/fonts/Inter.ttf").as_slice()),
                Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono.ttf").as_slice()),
            ])
            .expect("failed to load bundled fonts");

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

        let page = if std::env::args().any(|arg| arg == "--settings") {
            Page::Settings
        } else {
            Page::Today
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
            cx.new(|_| view::make(store.clone(), data_dir.clone(), page))
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
