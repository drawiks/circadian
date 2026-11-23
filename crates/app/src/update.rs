use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use ureq::Agent;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const FEED_URL: &str =
    "https://github.com/drawiks/circadian/releases/latest/download/latest.json";
pub const FEED_SIG_URL: &str =
    "https://github.com/drawiks/circadian/releases/latest/download/latest.json.minisig";

const PUBLIC_KEY: &str = include_str!("../assets/minisign.pub");
const REPO_PATH: &str = "drawiks/circadian";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 16 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
const USER_AGENT: &str = concat!("Circadian/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("{0}")]
    Message(String),
    #[error("HTTP: {0}")]
    Http(#[from] ureq::Error),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("signature: {0}")]
    Signature(String),
}

pub type Result<T> = std::result::Result<T, UpdateError>;

#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    schema_version: u32,
    version: String,
    tag: String,
    published_at: String,
    #[serde(default)]
    notes_url: Option<String>,
    installer: Artifact,
    #[serde(default)]
    portable: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub notes_url: Option<String>,
    pub installer: Artifact,
    pub portable: Option<Artifact>,
}

#[derive(Debug, Clone)]
pub struct Downloaded {
    pub dir: PathBuf,
    pub path: PathBuf,
    pub artifact: Artifact,
}

pub fn check() -> Result<Option<UpdateInfo>> {
    check_feed(FEED_URL, FEED_SIG_URL, PUBLIC_KEY)
}

fn check_feed(feed_url: &str, sig_url: &str, public_key_text: &str) -> Result<Option<UpdateInfo>> {
    let agent = http_agent(Duration::from_secs(30));
    let manifest_bytes = fetch_bytes(&agent, feed_url, MAX_MANIFEST_BYTES)?;
    let signature_bytes = fetch_bytes(&agent, sig_url, MAX_SIGNATURE_BYTES)?;
    let signature_text = String::from_utf8(signature_bytes)
        .map_err(|_| UpdateError::Signature("signature is not UTF-8".into()))?;

    verify_signature(public_key_text, &manifest_bytes, &signature_text)?;

    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    validate_manifest(&manifest)?;

    let signature = Signature::decode(&signature_text)
        .map_err(|error| UpdateError::Signature(error.to_string()))?;
    let expected_comment = format!("circadian stable {}", manifest.version);
    if !signature
        .trusted_comment()
        .trim()
        .starts_with(&expected_comment)
    {
        return Err(UpdateError::Signature(
            "manifest signature comment does not match the version".into(),
        ));
    }

    let current = Version::parse(CURRENT_VERSION)
        .map_err(|error| UpdateError::Message(format!("current version: {error}")))?;
    let latest = Version::parse(&manifest.version)
        .map_err(|error| UpdateError::Message(format!("release version: {error}")))?;
    if latest <= current {
        return Ok(None);
    }

    Ok(Some(UpdateInfo {
        version: manifest.version,
        notes_url: manifest.notes_url,
        installer: manifest.installer,
        portable: manifest.portable,
    }))
}

fn verify_signature(public_key_text: &str, payload: &[u8], signature_text: &str) -> Result<()> {
    let public_key = PublicKey::decode(public_key_text)
        .map_err(|error| UpdateError::Signature(error.to_string()))?;
    let signature = Signature::decode(signature_text)
        .map_err(|error| UpdateError::Signature(error.to_string()))?;
    public_key
        .verify(payload, &signature, false)
        .map_err(|error| UpdateError::Signature(error.to_string()))
}

fn validate_manifest(manifest: &Manifest) -> Result<()> {
    if manifest.schema_version != 1 {
        return Err(UpdateError::Message(
            "unsupported update manifest schema".into(),
        ));
    }
    if manifest.tag != format!("v{}", manifest.version) {
        return Err(UpdateError::Message(
            "update manifest tag does not match its version".into(),
        ));
    }
    if manifest.published_at.trim().is_empty() {
        return Err(UpdateError::Message(
            "update manifest is missing published_at".into(),
        ));
    }
    validate_artifact(&manifest.installer)?;
    if let Some(portable) = &manifest.portable {
        validate_artifact(portable)?;
    }
    if let Some(url) = &manifest.notes_url {
        validate_https_url(url, "notes URL")?;
    }
    Ok(())
}

fn validate_artifact(artifact: &Artifact) -> Result<()> {
    let prefix = format!("https://github.com/{REPO_PATH}/releases/");
    if !artifact.url.starts_with(&prefix) {
        return Err(UpdateError::Message(
            "update artifact URL is not from the Circadian GitHub releases".into(),
        ));
    }
    validate_https_url(&artifact.url, "artifact URL")?;
    if artifact.sha256.len() != 64 || !artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(UpdateError::Message(
            "update artifact has an invalid sha256".into(),
        ));
    }
    if artifact.size == 0 || artifact.size > MAX_ARTIFACT_BYTES {
        return Err(UpdateError::Message(
            "update artifact has an invalid size".into(),
        ));
    }
    Ok(())
}

fn validate_https_url(url: &str, what: &str) -> Result<()> {
    if !url.starts_with("https://") || url.len() > 2048 {
        return Err(UpdateError::Message(format!("invalid {what}")));
    }
    if url.chars().any(char::is_whitespace)
        || url.contains("..")
        || url.contains('\\')
        || url.contains('\0')
    {
        return Err(UpdateError::Message(format!("invalid {what}")));
    }
    Ok(())
}

pub fn download(info: &UpdateInfo, mut progress: impl FnMut(u64, u64)) -> Result<Downloaded> {
    let artifact = info.installer.clone();
    let agent = http_agent(Duration::from_secs(15 * 60));
    let response = agent
        .get(&artifact.url)
        .header("User-Agent", USER_AGENT)
        .call()?;

    if let Some(length) = response.body().content_length() {
        if length != artifact.size {
            return Err(UpdateError::Message(
                "update installer size does not match the manifest".into(),
            ));
        }
    }

    let dir = temp_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("Circadian-Setup-{}.exe", info.version));
    let result = (|| {
        let mut file = File::create(&path)?;
        let mut reader = response.into_body().into_reader();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut hasher = Sha256::new();
        let mut downloaded = 0u64;

        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            downloaded += read as u64;
            if downloaded > artifact.size {
                return Err(UpdateError::Message(
                    "update installer is larger than the manifest".into(),
                ));
            }
            file.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
            progress(downloaded, artifact.size);
        }
        file.sync_all()?;

        if downloaded != artifact.size {
            return Err(UpdateError::Message(
                "update installer size does not match the manifest".into(),
            ));
        }
        let digest = hex(&hasher.finalize());
        if !digest.eq_ignore_ascii_case(&artifact.sha256) {
            return Err(UpdateError::Message(
                "update installer checksum does not match the manifest".into(),
            ));
        }
        Ok(())
    })();

    if let Err(error) = result {
        let _ = fs::remove_dir_all(&dir);
        return Err(error);
    }

    Ok(Downloaded {
        dir,
        path,
        artifact,
    })
}

pub fn is_installed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .map(|dir| dir.join("unins000.exe").exists())
        .unwrap_or(false)
}

pub fn launch_installer(downloaded: &Downloaded) -> Result<()> {
    let current = std::env::current_exe()?;
    let install_dir = current
        .parent()
        .ok_or_else(|| UpdateError::Message("could not find install directory".into()))?;
    let helper = downloaded.dir.join("circadian-updater.exe");
    fs::copy(&current, &helper)?;
    let log = downloaded.dir.join("installer.log");

    Command::new(&helper)
        .arg("--apply-update")
        .arg("--installer")
        .arg(&downloaded.path)
        .arg("--sha256")
        .arg(&downloaded.artifact.sha256)
        .arg("--size")
        .arg(downloaded.artifact.size.to_string())
        .arg("--install-dir")
        .arg(install_dir)
        .arg("--app")
        .arg(&current)
        .arg("--parent")
        .arg(std::process::id().to_string())
        .arg("--log")
        .arg(&log)
        .spawn()?;
    Ok(())
}

pub fn cleanup_stale() {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("circadian-update-") {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified < cutoff)
            .unwrap_or(false);
        if old {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(windows)]
struct HelperArgs {
    installer: PathBuf,
    sha256: String,
    size: u64,
    install_dir: PathBuf,
    app: PathBuf,
    parent: u32,
    log: PathBuf,
}

#[cfg(windows)]
fn parse_helper_args(args: &[String]) -> Result<HelperArgs> {
    let mut installer = None;
    let mut sha256 = None;
    let mut size = None;
    let mut install_dir = None;
    let mut app = None;
    let mut parent = None;
    let mut log = None;

    let mut index = 0;
    while index < args.len() {
        let key = args[index].as_str();
        let next = || {
            args.get(index + 1).cloned().ok_or_else(|| {
                UpdateError::Message(format!("missing value for update helper argument {key}"))
            })
        };
        if key == "--apply-update" {
            index += 1;
            continue;
        }
        match key {
            "--installer" => installer = Some(PathBuf::from(next()?)),
            "--sha256" => sha256 = Some(next()?),
            "--size" => {
                size = Some(
                    next()?
                        .parse::<u64>()
                        .map_err(|_| UpdateError::Message("invalid update size".into()))?,
                )
            }
            "--install-dir" => install_dir = Some(PathBuf::from(next()?)),
            "--app" => app = Some(PathBuf::from(next()?)),
            "--parent" => {
                parent = Some(
                    next()?
                        .parse::<u32>()
                        .map_err(|_| UpdateError::Message("invalid parent pid".into()))?,
                )
            }
            "--log" => log = Some(PathBuf::from(next()?)),
            other => {
                return Err(UpdateError::Message(format!(
                    "unknown update helper argument: {other}"
                )))
            }
        }
        index += 2;
    }

    Ok(HelperArgs {
        installer: installer
            .ok_or_else(|| UpdateError::Message("missing update installer".into()))?,
        sha256: sha256.ok_or_else(|| UpdateError::Message("missing update sha256".into()))?,
        size: size.ok_or_else(|| UpdateError::Message("missing update size".into()))?,
        install_dir: install_dir
            .ok_or_else(|| UpdateError::Message("missing install directory".into()))?,
        app: app.ok_or_else(|| UpdateError::Message("missing app path".into()))?,
        parent: parent.ok_or_else(|| UpdateError::Message("missing parent pid".into()))?,
        log: log.ok_or_else(|| UpdateError::Message("missing installer log path".into()))?,
    })
}

#[cfg(windows)]
pub fn run_apply_update(args: &[String]) -> ! {
    match apply_update(&args[1..]) {
        Ok(()) => std::process::exit(0),
        Err(error) => {
            show_error(&format!("Circadian update failed:\n{error}"));
            std::process::exit(1);
        }
    }
}

#[cfg(windows)]
fn apply_update(args: &[String]) -> Result<()> {
    let args = parse_helper_args(args)?;
    wait_for_parent(args.parent);
    verify_installer(&args.installer, &args.sha256, args.size)?;

    let mut command = Command::new(&args.installer);
    command
        .arg("/SP-")
        .arg("/SILENT")
        .arg("/NOCANCEL")
        .arg("/NORESTART")
        .arg("/NOCLOSEAPPLICATIONS")
        .arg(format!("/LOG={}", args.log.display()))
        .arg(format!("/DIR={}", args.install_dir.display()));

    let status = command.status()?;
    let ok = status.success() || status.code() == Some(3010);
    if !ok {
        let _ = Command::new(&args.app).spawn();
        return Err(UpdateError::Message(format!(
            "installer exited with {:?}; log: {}",
            status.code(),
            args.log.display()
        )));
    }

    let _ = fs::remove_file(&args.installer);
    Command::new(&args.app).spawn()?;
    Ok(())
}

#[cfg(windows)]
fn verify_installer(path: &Path, expected: &str, expected_size: u64) -> Result<()> {
    let metadata = fs::metadata(path)?;
    if metadata.len() != expected_size {
        return Err(UpdateError::Message(
            "downloaded installer size changed before install".into(),
        ));
    }
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if !hex(&hasher.finalize()).eq_ignore_ascii_case(expected) {
        return Err(UpdateError::Message(
            "downloaded installer checksum changed before install".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn wait_for_parent(pid: u32) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    if let Ok(handle) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        unsafe {
            let _ = WaitForSingleObject(handle, 30_000);
            let _ = CloseHandle(handle);
        }
    } else {
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(windows)]
fn show_error(text: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

    let text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let caption: Vec<u16> = "Circadian"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_ICONERROR | MB_OK,
        );
    }
}

#[cfg(not(windows))]
pub fn run_apply_update(_args: &[String]) -> ! {
    eprintln!("Circadian updates are only supported on Windows");
    std::process::exit(1);
}

fn http_agent(timeout: Duration) -> Agent {
    Agent::new_with_config(
        Agent::config_builder()
            .timeout_global(Some(timeout))
            .https_only(true)
            .max_redirects(10)
            .build(),
    )
}

fn fetch_bytes(agent: &Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    let mut response = agent.get(url).header("User-Agent", USER_AGENT).call()?;
    Ok(response
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()?)
}

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "circadian-update-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ))
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PUBLIC_KEY: &str = "untrusted comment: minisign public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const TEST_SIGNATURE: &str = "untrusted comment: signature from minisign secret key
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=
trusted comment: timestamp:1633700835\tfile:test\tprehashed
wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==";

    fn manifest() -> Manifest {
        serde_json::from_str(
            r#"{
                "schema_version": 1,
                "version": "0.3.0",
                "tag": "v0.3.0",
                "published_at": "2026-11-23T12:00:00Z",
                "notes_url": "https://github.com/drawiks/circadian/releases/tag/v0.3.0",
                "installer": {
                    "url": "https://github.com/drawiks/circadian/releases/download/v0.3.0/Circadian-Setup-0.3.0.exe",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "size": 8000000
                }
            }"#,
        )
        .expect("valid manifest fixture")
    }

    #[test]
    fn app_public_key_parses() {
        PublicKey::decode(PUBLIC_KEY).expect("bundled update public key");
    }

    #[test]
    fn verifies_minisign_fixture() {
        verify_signature(TEST_PUBLIC_KEY, b"test", TEST_SIGNATURE).expect("valid signature");
        assert!(verify_signature(TEST_PUBLIC_KEY, b"other", TEST_SIGNATURE).is_err());
    }

    #[test]
    fn validates_manifest() {
        validate_manifest(&manifest()).expect("valid manifest");

        let mut bad_version = manifest();
        bad_version.tag = "v0.2.9".into();
        assert!(validate_manifest(&bad_version).is_err());

        let mut bad_digest = manifest();
        bad_digest.installer.sha256 = "nope".into();
        assert!(validate_manifest(&bad_digest).is_err());

        let mut bad_url = manifest();
        bad_url.installer.url = "https://example.com/setup.exe".into();
        assert!(validate_manifest(&bad_url).is_err());
    }

    #[test]
    fn compares_versions() {
        let current = Version::parse("0.2.0").unwrap();
        assert!(Version::parse("0.3.0").unwrap() > current);
        assert!(Version::parse("0.2.0").unwrap() <= current);
    }

    #[test]
    fn hashes_bytes() {
        assert_eq!(
            hex(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
