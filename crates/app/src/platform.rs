use std::path::PathBuf;
use std::process::Command;

pub fn data_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(value).join("Circadian")
    } else if let Some(value) = std::env::var_os("APPDATA") {
        PathBuf::from(value).join("Circadian")
    } else {
        PathBuf::from("Circadian")
    }
}

pub fn db_path() -> PathBuf {
    data_dir().join("circadian.db")
}

pub fn daemon_path() -> Option<PathBuf> {
    let current = std::env::current_exe().ok()?;
    let directory = current.parent()?;
    for name in ["circadiand.exe", "circadian-daemon.exe"] {
        let candidate = directory.join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

pub fn start_daemon() {
    let Some(path) = daemon_path() else {
        return;
    };
    let mut command = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let _ = command.spawn();
}

pub fn set_autostart(enabled: bool) {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;
        let key_path = r"Software\Microsoft\Windows\CurrentVersion\Run";
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(key_path) else {
            return;
        };
        if enabled {
            let Some(path) = daemon_path() else {
                return;
            };
            let value = format!("\"{}\"", path.display());
            let _ = key.0.set_value("Circadian", &value);
        } else {
            let _ = key.0.delete_value("Circadian");
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
    }
}

pub fn open_folder() {
    let dir = data_dir();
    let _ = std::fs::create_dir_all(&dir);
    #[cfg(windows)]
    {
        let _ = Command::new("explorer").arg(dir).spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(dir).spawn();
    }
}
