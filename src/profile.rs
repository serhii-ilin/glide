//! On-disk profile: settings, session, navigation log, and bookmarks.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

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
    pub js_enabled: bool,
    pub zoom_percent: i32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            js_enabled: true,
            zoom_percent: 100,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Session {
    pub entries: Vec<String>,
    pub index: usize,
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
        let mut session: Session = self.read_json(SESSION_FILE, Session::default());
        if !session.entries.is_empty() {
            let max = session.entries.len() - 1;
            session.index = session.index.min(max);
        } else {
            session.index = 0;
        }
        session
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
        writeln!(file, "{line}").map_err(|err| format!("history write: {err}"))?;
        file.sync_all()
            .map_err(|err| format!("history fsync: {err}"))?;
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
        let tmp = self.dir.join(format!("{name}.tmp"));
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
        eprintln!(
            "glide: ignoring corrupt profile file {} ({err})",
            path.display()
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
    if session.entries.is_empty() {
        let url = DEFAULT_START_URL.to_string();
        return (url.clone(), vec![url], 0);
    }
    let index = session.index.min(session.entries.len() - 1);
    let current = session.entries[index].clone();
    (current, session.entries.clone(), index)
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

pub fn is_error_page_title(title: &str) -> bool {
    title.contains("Can't open page")
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
        };
        p.save_settings(&settings).expect("save");
        assert_eq!(p.load_settings(), settings);
    }

    #[test]
    fn session_round_trip_and_index_clamped() {
        let p = temp_profile();
        let session = Session {
            entries: vec!["https://a/".into(), "https://b/".into()],
            index: 99,
        };
        p.save_session(&session).expect("save");
        let loaded = p.load_session();
        assert_eq!(loaded.entries, session.entries);
        assert_eq!(loaded.index, 1);
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
        assert!(!p.dir().join(format!("{SETTINGS_FILE}.tmp")).exists());
        assert!(p.dir().join(SETTINGS_FILE).exists());
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
        let session = Session {
            entries: vec!["https://old/".into()],
            index: 0,
        };
        let (url, entries, index) = window_start_state(Some("https://new/"), &session);
        assert_eq!(url, "https://new/");
        assert_eq!(entries, vec!["https://new/"]);
        assert_eq!(index, 0);
    }

    #[test]
    fn no_url_restores_session() {
        let session = Session {
            entries: vec!["https://a/".into(), "https://b/".into()],
            index: 1,
        };
        let (url, entries, index) = window_start_state(None, &session);
        assert_eq!(url, "https://b/");
        assert_eq!(entries, session.entries);
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
        };
        assert!(!effective_js_enabled(true, &saved));
        assert!(saved.js_enabled);
    }
}
