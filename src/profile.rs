//! On-disk profile: settings, session, navigation log, and bookmarks.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::devtools::{DEFAULT_PANEL_PX, DevToolsDock};
use crate::logging;
pub use crate::tabs::{Session, TabSession};
use crate::zoom::PageZoom;

const SETTINGS_FILE: &str = "settings.json";
const SESSION_FILE: &str = "session.json";
const HISTORY_FILE: &str = "history.jsonl";
const BOOKMARKS_FILE: &str = "bookmarks.json";

static CORRUPT_LOG: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// User data directory and persistence helpers.
#[derive(Debug, Clone)]
pub struct Profile {
    dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_js_enabled")]
    pub js_enabled: bool,
    #[serde(default = "default_zoom_percent")]
    pub zoom_percent: i32,
    #[serde(default)]
    pub devtools_dock: DevToolsDock,
    #[serde(default = "default_devtools_size_px")]
    pub devtools_size_px: i32,
}

fn default_devtools_size_px() -> i32 {
    DEFAULT_PANEL_PX
}

fn default_js_enabled() -> bool {
    true
}

fn default_zoom_percent() -> i32 {
    100
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            js_enabled: true,
            zoom_percent: 100,
            devtools_dock: DevToolsDock::default(),
            devtools_size_px: default_devtools_size_px(),
        }
    }
}

/// Session files written before tabs existed: one history stack in one window.
#[derive(Debug, Deserialize)]
struct LegacySession {
    #[serde(default)]
    entries: Vec<String>,
    #[serde(default)]
    index: usize,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SessionOnDisk {
    V2(Session),
    Legacy(LegacySession),
}

impl SessionOnDisk {
    fn into_session(self) -> Session {
        match self {
            Self::V2(mut session) => {
                session.validate();
                session
            }
            Self::Legacy(legacy) => Session::from_legacy(legacy.entries, legacy.index),
        }
    }
}

/// Disk-backed profile, or disabled when the data dir is unavailable.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    inner: Option<Profile>,
}

impl ProfileStore {
    pub fn open() -> Self {
        match Profile::open() {
            Ok(profile) => Self {
                inner: Some(profile),
            },
            Err(err) => {
                logging::warn(
                    "profile",
                    "profile unavailable; running without persistence",
                    &[
                        ("error", err),
                        ("hint", "Check permissions or set GLIDE_PROFILE_DIR.".into()),
                    ],
                );
                Self { inner: None }
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.is_some()
    }

    fn profile(&self) -> Option<&Profile> {
        self.inner.as_ref()
    }

    pub fn load_settings(&self) -> Settings {
        self.profile()
            .map(Profile::load_settings)
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        match self.profile() {
            Some(p) => p.save_settings(settings),
            None => Ok(()),
        }
    }

    pub fn load_session(&self) -> Session {
        self.profile()
            .map(Profile::load_session)
            .unwrap_or_default()
    }

    pub fn save_session(&self, session: &Session) -> Result<(), String> {
        match self.profile() {
            Some(p) => p.save_session(session),
            None => Ok(()),
        }
    }

    pub fn append_history(&self, entry: &HistoryEntry) -> Result<(), String> {
        match self.profile() {
            Some(p) => p.append_history(entry),
            None => Ok(()),
        }
    }

    pub fn load_bookmarks(&self) -> Vec<Bookmark> {
        self.profile()
            .map(Profile::load_bookmarks)
            .unwrap_or_default()
    }

    pub fn save_bookmarks(&self, bookmarks: &[Bookmark]) -> Result<(), String> {
        match self.profile() {
            Some(p) => p.save_bookmarks(bookmarks),
            None => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingHistoryCommand {
    Go,
    Back,
    Forward,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryCommitKind {
    Completed,
    LinkNavigation,
    Redirect,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryLogState {
    pub cli_start_url: Option<String>,
    pub restore_start_url: Option<String>,
    pub cli_start_logged: bool,
    pub restore_replay_seen: bool,
    pub pending_command: Option<PendingHistoryCommand>,
}

/// Whether a committed navigation should append one line to `history.jsonl`.
pub fn should_append_history_log(
    state: &mut HistoryLogState,
    kind: HistoryCommitKind,
    url: &str,
) -> bool {
    if !should_persist_url(url) {
        return false;
    }
    match kind {
        HistoryCommitKind::LinkNavigation | HistoryCommitKind::Redirect => true,
        HistoryCommitKind::Completed => {
            if state.pending_command == Some(PendingHistoryCommand::Reload) {
                state.pending_command = None;
                return false;
            }
            if let Some(ref cli) = state.cli_start_url
                && !state.cli_start_logged
                && url == cli.as_str()
            {
                state.cli_start_logged = true;
                state.pending_command = None;
                return true;
            }
            if let Some(ref restore) = state.restore_start_url
                && !state.restore_replay_seen
                && url == restore.as_str()
            {
                state.restore_replay_seen = true;
                state.pending_command = None;
                return false;
            }
            let log = matches!(
                state.pending_command,
                Some(PendingHistoryCommand::Go)
                    | Some(PendingHistoryCommand::Back)
                    | Some(PendingHistoryCommand::Forward)
            );
            state.pending_command = None;
            log
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ts_unix: u64,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
    pub added_unix: u64,
}

impl Profile {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn default_dir() -> Result<PathBuf, String> {
        if let Ok(override_dir) = std::env::var("GLIDE_PROFILE_DIR") {
            let path = PathBuf::from(override_dir);
            if path.as_os_str().is_empty() {
                return Err("GLIDE_PROFILE_DIR is empty".to_string());
            }
            return Ok(path);
        }
        let dirs = ProjectDirs::from("", "", "Glide")
            .ok_or_else(|| "could not resolve Glide profile directory".to_string())?;
        Ok(dirs.data_dir().to_path_buf())
    }

    pub fn open() -> Result<Self, String> {
        let dir = Self::default_dir()?;
        fs::create_dir_all(&dir).map_err(|err| format!("profile mkdir: {err}"))?;
        Ok(Self::new(dir))
    }

    pub fn load_settings(&self) -> Settings {
        self.read_json(SETTINGS_FILE, Settings::default())
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        self.write_json(SETTINGS_FILE, settings)
    }

    pub fn load_session(&self) -> Session {
        let raw: SessionOnDisk =
            self.read_json(SESSION_FILE, SessionOnDisk::V2(Session::default()));
        raw.into_session()
    }

    pub fn save_session(&self, session: &Session) -> Result<(), String> {
        self.write_json(SESSION_FILE, session)
    }

    pub fn append_history(&self, entry: &HistoryEntry) -> Result<(), String> {
        let path = self.dir.join(HISTORY_FILE);
        fs::create_dir_all(&self.dir).map_err(|err| format!("profile mkdir: {err}"))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|err| format!("history append: {err}"))?;
        let line = serde_json::to_string(entry).map_err(|err| format!("history encode: {err}"))?;
        file.write_all(format!("{line}\n").as_bytes())
            .map_err(|err| format!("history write: {err}"))?;
        Ok(())
    }

    pub fn load_bookmarks(&self) -> Vec<Bookmark> {
        self.read_json(BOOKMARKS_FILE, Vec::new())
    }

    pub fn save_bookmarks(&self, bookmarks: &[Bookmark]) -> Result<(), String> {
        self.write_json(BOOKMARKS_FILE, &bookmarks.to_vec())
    }

    pub fn toggle_bookmark(&self, url: &str, title: &str, now_unix: u64) -> Result<bool, String> {
        let mut bookmarks = self.load_bookmarks();
        if let Some(pos) = bookmarks.iter().position(|b| b.url == url) {
            bookmarks.remove(pos);
            self.save_bookmarks(&bookmarks)?;
            Ok(false)
        } else {
            bookmarks.push(Bookmark {
                url: url.to_string(),
                title: title.to_string(),
                added_unix: now_unix,
            });
            self.save_bookmarks(&bookmarks)?;
            Ok(true)
        }
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.load_bookmarks().iter().any(|b| b.url == url)
    }

    fn read_json<T>(&self, name: &str, default: T) -> T
    where
        T: for<'de> Deserialize<'de>,
    {
        let path = self.dir.join(name);
        let Ok(bytes) = fs::read(&path) else {
            return default;
        };
        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => value,
            Err(err) => {
                log_corrupt_once(&path, &err);
                default
            }
        }
    }

    fn write_json<T>(&self, name: &str, value: &T) -> Result<(), String>
    where
        T: Serialize,
    {
        fs::create_dir_all(&self.dir).map_err(|err| format!("profile mkdir: {err}"))?;
        let path = self.dir.join(name);
        let tmp = self.dir.join(format!("{name}.{}.tmp", std::process::id()));
        let json = serde_json::to_vec_pretty(value).map_err(|err| format!("json encode: {err}"))?;
        {
            let mut file = File::create(&tmp).map_err(|err| format!("profile write: {err}"))?;
            file.write_all(&json)
                .map_err(|err| format!("profile write: {err}"))?;
            file.sync_all()
                .map_err(|err| format!("profile fsync: {err}"))?;
        }
        fs::rename(&tmp, &path).map_err(|err| format!("profile rename: {err}"))?;
        Ok(())
    }
}

fn log_corrupt_once(path: &Path, err: &serde_json::Error) {
    let mut guard = CORRUPT_LOG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let set = guard.get_or_insert_with(HashSet::new);
    if set.insert(path.to_path_buf()) {
        logging::warn(
            "profile",
            "ignoring corrupt profile file",
            &[
                ("path", path.display().to_string()),
                ("error", err.to_string()),
                (
                    "hint",
                    "The file was left in place; defaults are used for this run.".into(),
                ),
            ],
        );
    }
}

/// Choose scripting for this run: `--no-js` wins without touching saved settings.
pub fn effective_js_enabled(cli_no_js: bool, saved: &Settings) -> bool {
    if cli_no_js { false } else { saved.js_enabled }
}

pub const DEFAULT_START_URL: &str = "https://example.com/";

/// Resolve the first history entry and stack position from CLI vs. saved session.
pub fn window_start_state(
    cli_url: Option<&str>,
    session: &Session,
) -> (String, Vec<String>, usize) {
    if let Some(url) = cli_url {
        return (url.to_string(), vec![url.to_string()], 0);
    }
    let Some(tab) = session
        .tabs
        .get(session.active_index)
        .or(session.tabs.first())
    else {
        let url = DEFAULT_START_URL.to_string();
        return (url.clone(), vec![url], 0);
    };
    let entries = if tab.entries.is_empty() {
        vec![tab.url.clone()]
    } else {
        tab.entries.clone()
    };
    let index = tab.index.min(entries.len().saturating_sub(1));
    let current = entries
        .get(index)
        .cloned()
        .unwrap_or_else(|| tab.url.clone());
    (current, entries, index)
}

pub fn settings_from_saved(saved: &Settings) -> (Settings, PageZoom) {
    let mut settings = saved.clone();
    settings.zoom_percent = settings.zoom_percent.clamp(30, 300);
    let mut zoom = PageZoom::new();
    zoom.set_percent(settings.zoom_percent);
    (settings, zoom)
}

/// Build the internal bookmarks listing page (HTML-escaped).
pub fn bookmarks_page_html(bookmarks: &[Bookmark]) -> String {
    let mut items = String::new();
    for bookmark in bookmarks.iter().rev() {
        let url = escape_html(&bookmark.url);
        let title = escape_html(&bookmark.title);
        let label = if title.is_empty() { url.clone() } else { title };
        items.push_str(&format!(
            r#"<li><a href="{url}">{label}</a> <span class="url">{url}</span></li>
"#
        ));
    }
    if items.is_empty() {
        items.push_str("<li><em>No bookmarks yet. Press Cmd/Ctrl+D on a page.</em></li>\n");
    }
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Bookmarks</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 2rem; color: #1c1915; }}
  h1 {{ font-size: 1.25rem; }}
  ul {{ padding-left: 1.25rem; }}
  .url {{ color: #6b6358; font-size: 0.85rem; display: block; word-break: break-all; }}
  a {{ color: #2563eb; text-decoration: none; }}
  a:hover {{ text-decoration: underline; }}
</style>
</head>
<body>
<h1>Bookmarks</h1>
<ul>
{items}</ul>
</body>
</html>
"#
    )
}

/// Internal keyboard-shortcuts page linked from the Help menu.
pub fn help_page_html() -> String {
    let rows = [
        ("Cmd/Ctrl+T", "New tab"),
        (
            "Cmd/Ctrl+W",
            "Close the active tab (the window closes with the last tab)",
        ),
        ("Cmd/Ctrl+Tab", "Next tab"),
        ("Cmd/Ctrl+Shift+Tab", "Previous tab"),
        ("Cmd+Shift+] / Cmd+Shift+[", "Next / previous tab on macOS"),
        ("Cmd/Ctrl+1…9", "Select a tab by position"),
        ("Cmd/Ctrl+L", "Focus the address bar"),
        ("Cmd/Ctrl+R or F5", "Reload"),
        ("Cmd/Ctrl+[ / ]", "Back / forward"),
        ("Cmd/Ctrl+F", "Find in page"),
        ("Cmd/Ctrl+= / - / 0", "Zoom in / out / reset (current tab)"),
        ("Cmd/Ctrl+D", "Bookmark the active tab"),
        ("Cmd/Ctrl+Shift+J", "Toggle page JavaScript (global)"),
        ("F12; Meta+Alt+I / Ctrl+Shift+I", "Toggle developer tools"),
        ("Esc (in DevTools)", "Close developer tools"),
        ("Cmd/Ctrl+Q", "Quit"),
    ];
    let mut items = String::new();
    for (key, action) in rows {
        items.push_str(&format!(
            "<tr><th>{}</th><td>{}</td></tr>\n",
            escape_html(key),
            escape_html(action)
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Keyboard shortcuts</title>
<style>
  body {{ font-family: system-ui, sans-serif; margin: 2rem; color: #1c1915; }}
  h1 {{ font-size: 1.25rem; }}
  table {{ border-collapse: collapse; }}
  th, td {{ border-bottom: 1px solid #e4ddd0; padding: 0.35rem 0.9rem; text-align: left; }}
  th {{ font-family: ui-monospace, monospace; white-space: nowrap; }}
</style>
</head>
<body>
<h1>Glide keyboard shortcuts</h1>
<table>
{items}</table>
</body>
</html>
"#
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// URLs that should not be logged or stored in session/history files.
pub fn should_persist_url(url: &str) -> bool {
    !url.starts_with("glide:") && !url.is_empty()
}

pub fn history_log_state_for_launch(
    cli_url: Option<&str>,
    start_url: &str,
    session: &Session,
) -> HistoryLogState {
    HistoryLogState {
        cli_start_url: cli_url.map(|u| u.to_string()),
        restore_start_url: if cli_url.is_none() && !session.tabs.is_empty() {
            Some(start_url.to_string())
        } else {
            None
        },
        ..HistoryLogState::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_profile() -> Profile {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("glide-profile-test-{n}"));
        let _ = fs::remove_dir_all(&dir);
        Profile::new(dir)
    }

    #[test]
    fn missing_files_give_defaults() {
        let p = temp_profile();
        assert_eq!(p.load_settings(), Settings::default());
        assert_eq!(p.load_session(), Session::default());
        assert!(p.load_bookmarks().is_empty());
    }

    #[test]
    fn settings_round_trip() {
        let p = temp_profile();
        let settings = Settings {
            js_enabled: false,
            zoom_percent: 150,
            devtools_dock: DevToolsDock::Right,
            devtools_size_px: 360,
        };
        p.save_settings(&settings).expect("save");
        assert_eq!(p.load_settings(), settings);
    }

    #[test]
    fn session_round_trip_and_index_clamped() {
        let p = temp_profile();
        let mut session = Session::from_legacy(vec!["https://a/".into(), "https://b/".into()], 99);
        session.tabs[0].index = 99;
        p.save_session(&session).expect("save");
        let loaded = p.load_session();
        assert_eq!(loaded.tabs[0].entries, session.tabs[0].entries);
        assert_eq!(loaded.tabs[0].index, 1);
    }

    #[test]
    fn session_v2_round_trip() {
        let p = temp_profile();
        let mut session = Session::default();
        session.tabs.push(TabSession {
            url: "https://a/".into(),
            entries: vec!["https://a/".into()],
            index: 0,
            title: "A".into(),
            find_query: "needle".into(),
            find_open: true,
            zoom_percent: 150,
        });
        session.tabs.push(TabSession {
            url: "https://b/".into(),
            entries: vec!["https://b/".into()],
            index: 0,
            title: "B".into(),
            find_query: String::new(),
            find_open: false,
            zoom_percent: 100,
        });
        session.active_index = 1;
        p.save_session(&session).expect("save");
        let loaded = p.load_session();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.active_index, 1);
        assert_eq!(loaded.tabs, session.tabs);
    }

    #[test]
    fn legacy_session_migrates_to_one_tab() {
        let p = temp_profile();
        fs::create_dir_all(p.dir()).expect("mkdir");
        fs::write(
            p.dir().join(SESSION_FILE),
            br#"{"entries":["https://a/","https://b/"],"index":1}"#,
        )
        .expect("write legacy session");
        let session = p.load_session();
        assert_eq!(session.version, 2);
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].entries, vec!["https://a/", "https://b/"]);
        assert_eq!(session.tabs[0].index, 1);
        assert_eq!(session.tabs[0].url, "https://b/");
    }

    #[test]
    fn corrupt_session_falls_back() {
        let p = temp_profile();
        fs::create_dir_all(p.dir()).expect("mkdir");
        fs::write(p.dir().join(SESSION_FILE), b"{not json").expect("write");
        assert_eq!(p.load_session(), Session::default());
    }

    #[test]
    fn corrupt_json_gives_defaults_and_is_not_overwritten_on_load() {
        let p = temp_profile();
        fs::create_dir_all(p.dir()).expect("mkdir");
        fs::write(p.dir().join(SETTINGS_FILE), b"{not json").expect("write");
        assert_eq!(p.load_settings(), Settings::default());
        assert!(p.dir().join(SETTINGS_FILE).exists());
        let raw = fs::read_to_string(p.dir().join(SETTINGS_FILE)).expect("read");
        assert_eq!(raw, "{not json");
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file() {
        let p = temp_profile();
        p.save_settings(&Settings::default()).expect("save");
        let tmp_name = format!("{SETTINGS_FILE}.{}.tmp", std::process::id());
        assert!(!p.dir().join(tmp_name).exists());
        assert!(p.dir().join(SETTINGS_FILE).exists());
    }

    #[test]
    fn settings_missing_field_keeps_others() {
        let p = temp_profile();
        fs::create_dir_all(p.dir()).expect("mkdir");
        fs::write(p.dir().join(SETTINGS_FILE), br#"{"js_enabled":false}"#).expect("write");
        assert_eq!(
            p.load_settings(),
            Settings {
                js_enabled: false,
                zoom_percent: 100,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn devtools_settings_default_on_legacy_file() {
        let p = temp_profile();
        fs::create_dir_all(p.dir()).expect("mkdir");
        fs::write(
            p.dir().join(SETTINGS_FILE),
            br#"{"js_enabled":true,"zoom_percent":125}"#,
        )
        .expect("write");
        let loaded = p.load_settings();
        assert_eq!(loaded.devtools_dock, DevToolsDock::default());
        assert_eq!(loaded.devtools_size_px, DEFAULT_PANEL_PX);
    }

    #[test]
    fn session_save_skips_internal_and_error_entries() {
        use crate::tabs::TabManager;

        let mut tabs = TabManager::new();
        tabs.add("https://a/");
        let tab = tabs.active_mut().expect("active tab");
        tab.nav.history_mut().push("glide://bookmarks".into());
        tab.nav.history_mut().push("https://b/".into());
        let mut failed = HashSet::new();
        failed.insert("https://b/".into());
        let session = Session::from_tabs(&tabs, &failed);
        assert_eq!(session.tabs[0].entries, vec!["https://a/"]);
        assert_eq!(session.tabs[0].index, 0);
    }

    #[test]
    fn history_log_skips_reload_and_restore_replay() {
        let mut state = HistoryLogState {
            restore_start_url: Some("https://saved/".into()),
            ..HistoryLogState::default()
        };
        assert!(!should_append_history_log(
            &mut state,
            HistoryCommitKind::Completed,
            "https://saved/"
        ));
        state.pending_command = Some(PendingHistoryCommand::Reload);
        assert!(!should_append_history_log(
            &mut state,
            HistoryCommitKind::Completed,
            "https://x/"
        ));
        state.pending_command = Some(PendingHistoryCommand::Back);
        assert!(should_append_history_log(
            &mut state,
            HistoryCommitKind::Completed,
            "https://older/"
        ));
    }

    #[test]
    fn history_log_cli_start_url() {
        let mut state = HistoryLogState {
            cli_start_url: Some("https://cli/".into()),
            ..HistoryLogState::default()
        };
        assert!(should_append_history_log(
            &mut state,
            HistoryCommitKind::Completed,
            "https://cli/"
        ));
    }

    #[test]
    fn history_log_appends_lines() {
        let p = temp_profile();
        let e1 = HistoryEntry {
            ts_unix: 1,
            url: "https://a/".into(),
            title: "A".into(),
        };
        let e2 = HistoryEntry {
            ts_unix: 2,
            url: "https://b/".into(),
            title: "B".into(),
        };
        p.append_history(&e1).expect("a");
        p.append_history(&e2).expect("b");
        let path = p.dir().join(HISTORY_FILE);
        let file = File::open(path).expect("open");
        let lines: Vec<_> = std::io::BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("https://a/"));
        assert!(lines[1].contains("https://b/"));
    }

    #[test]
    fn bookmark_toggle_adds_then_removes() {
        let p = temp_profile();
        let now = 42;
        assert!(p.toggle_bookmark("https://x/", "X", now).expect("add"));
        assert!(p.is_bookmarked("https://x/"));
        assert!(!p.toggle_bookmark("https://x/", "X", now).expect("remove"));
        assert!(!p.is_bookmarked("https://x/"));
    }

    #[test]
    fn cli_url_overrides_session() {
        let session = Session::from_legacy(vec!["https://old/".into()], 0);
        let (url, entries, index) = window_start_state(Some("https://new/"), &session);
        assert_eq!(url, "https://new/");
        assert_eq!(entries, vec!["https://new/"]);
        assert_eq!(index, 0);
    }

    #[test]
    fn no_url_restores_session() {
        let session = Session::from_legacy(vec!["https://a/".into(), "https://b/".into()], 1);
        let (url, entries, index) = window_start_state(None, &session);
        assert_eq!(url, "https://b/");
        assert_eq!(entries, vec!["https://a/", "https://b/"]);
        assert_eq!(index, 1);
    }

    #[test]
    fn empty_session_uses_default_page() {
        let (url, entries, index) = window_start_state(None, &Session::default());
        assert_eq!(url, DEFAULT_START_URL);
        assert_eq!(entries, vec![DEFAULT_START_URL]);
        assert_eq!(index, 0);
    }

    #[test]
    fn no_js_flag_does_not_overwrite_saved_setting() {
        let saved = Settings {
            js_enabled: true,
            zoom_percent: 100,
            ..Settings::default()
        };
        assert!(!effective_js_enabled(true, &saved));
        assert!(saved.js_enabled);
    }
}
