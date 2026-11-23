//! Origin-scoped Web Storage (`localStorage` / `sessionStorage`) with profile persistence.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

const STORAGE_FILE: &str = "local_storage.json";
pub const QUOTA_BYTES: usize = 5 * 1024 * 1024;

static GLOBAL: RwLock<Option<Arc<WebStorage>>> = RwLock::new(None);
static NEXT_OPAQUE_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

/// Unique id for opaque-origin storage partitions (stable across JsHost restarts).
pub fn allocate_opaque_document_id() -> u64 {
    NEXT_OPAQUE_DOCUMENT_ID.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
static GLOBAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageArea {
    Local,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Partition {
    Tuple {
        origin: String,
        tab_id: u64,
    },
    Opaque {
        tab_id: u64,
        opaque_document_id: u64,
    },
}

#[derive(Debug, Clone)]
pub struct StorageChange {
    pub area: StorageArea,
    pub key: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub page_url: String,
    pub origin: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedOrigin {
    #[serde(default)]
    entries: Vec<(String, String)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedFile {
    #[serde(default)]
    origins: HashMap<String, PersistedOrigin>,
}

#[derive(Debug, Clone, Default)]
struct OrderedStore {
    order: Vec<String>,
    values: HashMap<String, String>,
}

impl OrderedStore {
    fn from_legacy_map(map: BTreeMap<String, String>) -> Self {
        let order: Vec<String> = map.keys().cloned().collect();
        let values = map.into_iter().collect();
        Self { order, values }
    }

    fn from_persisted(origin: &PersistedOrigin) -> Self {
        if !origin.entries.is_empty() {
            let mut order = Vec::with_capacity(origin.entries.len());
            let mut values = HashMap::with_capacity(origin.entries.len());
            for (key, value) in &origin.entries {
                if !values.contains_key(key) {
                    order.push(key.clone());
                }
                values.insert(key.clone(), value.clone());
            }
            return Self { order, values };
        }
        Self::default()
    }

    fn to_persisted(&self) -> PersistedOrigin {
        PersistedOrigin {
            entries: self
                .order
                .iter()
                .filter_map(|k| self.values.get(k).map(|v| (k.clone(), v.clone())))
                .collect(),
        }
    }

    fn len(&self) -> usize {
        self.values.len()
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn get(&self, key: &str) -> Option<&String> {
        self.values.get(key)
    }

    fn insert(&mut self, key: String, value: String) {
        if !self.values.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.values.insert(key, value);
    }

    fn remove(&mut self, key: &str) -> Option<String> {
        let removed = self.values.remove(key)?;
        self.order.retain(|k| k != key);
        Some(removed)
    }

    fn clear(&mut self) {
        self.order.clear();
        self.values.clear();
    }

    fn key_at(&self, index: usize) -> Option<&String> {
        self.order.get(index)
    }

    fn iter_pairs(&self) -> impl Iterator<Item = (&String, &String)> {
        self.order
            .iter()
            .filter_map(|k| self.values.get(k).map(|v| (k, v)))
    }
}

/// Process-wide storage backing RPC from the JS worker.
#[derive(Debug)]
pub struct WebStorage {
    local_tuple: RwLock<HashMap<String, OrderedStore>>,
    local_opaque: RwLock<HashMap<u64, OrderedStore>>,
    session_tuple: RwLock<HashMap<(u64, String), OrderedStore>>,
    session_opaque: RwLock<HashMap<(u64, u64), OrderedStore>>,
    persist_path: Option<PathBuf>,
    persist_scheduled: AtomicBool,
}

impl WebStorage {
    pub fn empty() -> Self {
        Self {
            local_tuple: RwLock::new(HashMap::new()),
            local_opaque: RwLock::new(HashMap::new()),
            session_tuple: RwLock::new(HashMap::new()),
            session_opaque: RwLock::new(HashMap::new()),
            persist_path: None,
            persist_scheduled: AtomicBool::new(false),
        }
    }

    pub fn with_persist_path(path: PathBuf) -> Self {
        Self {
            local_tuple: RwLock::new(HashMap::new()),
            local_opaque: RwLock::new(HashMap::new()),
            session_tuple: RwLock::new(HashMap::new()),
            session_opaque: RwLock::new(HashMap::new()),
            persist_path: Some(path),
            persist_scheduled: AtomicBool::new(false),
        }
    }

    pub fn storage_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join(STORAGE_FILE)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let file = PersistedFile::read(path)?;
        let mut tuple = HashMap::new();
        for (origin, map) in file.origins {
            tuple.insert(origin, OrderedStore::from_persisted(&map));
        }
        Ok(Self {
            local_tuple: RwLock::new(tuple),
            local_opaque: RwLock::new(HashMap::new()),
            session_tuple: RwLock::new(HashMap::new()),
            session_opaque: RwLock::new(HashMap::new()),
            persist_path: Some(path.to_path_buf()),
            persist_scheduled: AtomicBool::new(false),
        })
    }

    pub fn install_global(store: Arc<Self>) {
        *GLOBAL.write().unwrap_or_else(|p| p.into_inner()) = Some(store);
    }

    pub fn global() -> Arc<Self> {
        if let Ok(guard) = GLOBAL.read()
            && let Some(store) = guard.as_ref()
        {
            return Arc::clone(store);
        }
        let mut guard = GLOBAL.write().unwrap_or_else(|p| p.into_inner());
        guard.get_or_insert_with(|| Arc::new(Self::empty())).clone()
    }

    pub fn ensure_process_global(profile_dir: Option<&Path>) {
        if GLOBAL.read().unwrap_or_else(|p| p.into_inner()).is_some() {
            return;
        }
        let mut guard = GLOBAL.write().unwrap_or_else(|p| p.into_inner());
        if guard.is_none() {
            *guard = Some(bootstrap_from_profile(profile_dir));
        }
    }

    /// Reload the in-memory store from disk (headless profile restart tests).
    pub fn reinit_from_profile(profile_dir: Option<&Path>) {
        let mut guard = GLOBAL.write().unwrap_or_else(|p| p.into_inner());
        *guard = Some(bootstrap_from_profile(profile_dir));
    }

    pub fn replace_global(store: Arc<Self>) {
        *GLOBAL.write().unwrap_or_else(|p| p.into_inner()) = Some(store);
    }

    pub fn reset_global() {
        *GLOBAL.write().unwrap_or_else(|p| p.into_inner()) = None;
    }

    #[cfg(test)]
    pub fn lock_global_for_test() -> std::sync::MutexGuard<'static, ()> {
        GLOBAL_LOCK.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = self.persist_path.as_ref() else {
            return Ok(());
        };
        let snapshot = {
            let guard = self.local_tuple.read().unwrap_or_else(|p| p.into_inner());
            PersistedFile {
                origins: guard
                    .iter()
                    .map(|(origin, map)| (origin.clone(), map.to_persisted()))
                    .collect(),
            }
        };
        match snapshot.write_atomic(path) {
            Ok(()) => Ok(()),
            Err(err) => {
                crate::logging::warn(
                    "profile.storage",
                    "local_storage save failed",
                    &[
                        ("error_kind", "io".into()),
                        ("detail", err.clone()),
                        ("hint", "check profile dir permissions (F-081)".into()),
                    ],
                );
                Err(err)
            }
        }
    }

    /// Block until any debounced persist finishes and write `localStorage` to disk.
    pub fn flush_persist(&self) {
        for _ in 0..200 {
            if !self.persist_scheduled.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = self.save();
    }

    fn schedule_persist_local(self: &Arc<Self>) {
        if self.persist_path.is_none() {
            return;
        }
        if self
            .persist_scheduled
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let store = Arc::clone(self);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            let _ = store.save();
            store.persist_scheduled.store(false, Ordering::Release);
        });
    }

    fn partition(
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
    ) -> Result<Partition, String> {
        let url = Url::parse(page_url).map_err(|e| e.to_string())?;
        if is_opaque_storage_origin(&url) {
            return Ok(Partition::Opaque {
                tab_id,
                opaque_document_id,
            });
        }
        Ok(Partition::Tuple {
            origin: url.origin().ascii_serialization(),
            tab_id,
        })
    }

    pub(crate) fn utf16_len(s: &str) -> usize {
        s.encode_utf16().count()
    }

    fn map_size_utf16(map: &OrderedStore) -> usize {
        map.iter_pairs()
            .map(|(k, v)| Self::utf16_len(k) + Self::utf16_len(v))
            .sum()
    }

    fn check_quota(map: &OrderedStore, key: &str, new_value: &str) -> Result<(), String> {
        let old_len = map
            .get(key)
            .map(|v| Self::utf16_len(key) + Self::utf16_len(v))
            .unwrap_or(0);
        let new_len = Self::utf16_len(key) + Self::utf16_len(new_value);
        let total = Self::map_size_utf16(map) + new_len.saturating_sub(old_len);
        if total > QUOTA_BYTES {
            return Err("QuotaExceededError".into());
        }
        Ok(())
    }

    fn snapshot_pairs(&self, area: StorageArea, partition: &Partition) -> Vec<(String, String)> {
        self.with_map_read(area, partition, |map| {
            map.iter_pairs()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
    }

    pub fn snapshot_json(
        &self,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
    ) -> Result<serde_json::Value, String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        let local: serde_json::Map<String, serde_json::Value> = self
            .snapshot_pairs(StorageArea::Local, &partition)
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        let session: serde_json::Map<String, serde_json::Value> = self
            .snapshot_pairs(StorageArea::Session, &partition)
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        Ok(serde_json::json!({ "local": local, "session": session }))
    }

    fn with_map_mut<R>(
        &self,
        area: StorageArea,
        partition: &Partition,
        f: impl FnOnce(&mut OrderedStore) -> R,
    ) -> R {
        match (area, partition) {
            (StorageArea::Local, Partition::Tuple { origin, .. }) => {
                let mut guard = self.local_tuple.write().unwrap_or_else(|p| p.into_inner());
                f(guard.entry(origin.clone()).or_default())
            }
            (
                StorageArea::Local,
                Partition::Opaque {
                    opaque_document_id, ..
                },
            ) => {
                let mut guard = self.local_opaque.write().unwrap_or_else(|p| p.into_inner());
                f(guard.entry(*opaque_document_id).or_default())
            }
            (StorageArea::Session, Partition::Tuple { origin, tab_id }) => {
                let mut guard = self
                    .session_tuple
                    .write()
                    .unwrap_or_else(|p| p.into_inner());
                f(guard.entry((*tab_id, origin.clone())).or_default())
            }
            (
                StorageArea::Session,
                Partition::Opaque {
                    tab_id,
                    opaque_document_id,
                },
            ) => {
                let mut guard = self
                    .session_opaque
                    .write()
                    .unwrap_or_else(|p| p.into_inner());
                f(guard.entry((*tab_id, *opaque_document_id)).or_default())
            }
        }
    }

    fn with_map_read<R>(
        &self,
        area: StorageArea,
        partition: &Partition,
        f: impl FnOnce(&OrderedStore) -> R,
    ) -> R {
        let empty = OrderedStore::default();
        match (area, partition) {
            (StorageArea::Local, Partition::Tuple { origin, .. }) => {
                let guard = self.local_tuple.read().unwrap_or_else(|p| p.into_inner());
                let map = guard.get(origin).unwrap_or(&empty);
                f(map)
            }
            (
                StorageArea::Local,
                Partition::Opaque {
                    opaque_document_id, ..
                },
            ) => {
                let guard = self.local_opaque.read().unwrap_or_else(|p| p.into_inner());
                let map = guard.get(opaque_document_id).unwrap_or(&empty);
                f(map)
            }
            (StorageArea::Session, Partition::Tuple { origin, tab_id }) => {
                let guard = self.session_tuple.read().unwrap_or_else(|p| p.into_inner());
                let map = guard.get(&(*tab_id, origin.clone())).unwrap_or(&empty);
                f(map)
            }
            (
                StorageArea::Session,
                Partition::Opaque {
                    tab_id,
                    opaque_document_id,
                },
            ) => {
                let guard = self
                    .session_opaque
                    .read()
                    .unwrap_or_else(|p| p.into_inner());
                let map = guard.get(&(*tab_id, *opaque_document_id)).unwrap_or(&empty);
                f(map)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get_item(
        &self,
        area: StorageArea,
        key: &str,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
    ) -> Result<Option<String>, String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        Ok(self.with_map_read(area, &partition, |map| map.get(key).cloned()))
    }

    fn emit_storage_change(
        partition: &Partition,
        tab_id: u64,
        source_generation: u64,
        change: StorageChange,
    ) {
        if let Partition::Tuple { origin, .. } = partition {
            crate::js::storage_events::notify_other_listeners(
                origin,
                tab_id,
                source_generation,
                &change,
            );
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors Web Storage setter inputs from DOM RPC
    pub fn set_item(
        &self,
        area: StorageArea,
        key: &str,
        value: &str,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
        source_generation: u64,
    ) -> Result<(), String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        let key = key.to_string();
        let value = value.to_string();
        let old_value = self.with_map_read(area, &partition, |map| map.get(&key).cloned());
        if old_value.as_deref() == Some(value.as_str()) {
            return Ok(());
        }
        self.with_map_mut(area, &partition, |map| {
            Self::check_quota(map, &key, &value)?;
            map.insert(key.clone(), value.clone());
            Ok::<(), String>(())
        })?;
        if area == StorageArea::Local {
            self.maybe_persist_local();
        }
        let origin = match &partition {
            Partition::Tuple { origin, .. } => origin.clone(),
            _ => String::new(),
        };
        let change = StorageChange {
            area,
            key: Some(key),
            old_value,
            new_value: Some(value),
            page_url: page_url.to_string(),
            origin,
        };
        Self::emit_storage_change(&partition, tab_id, source_generation, change);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_item(
        &self,
        area: StorageArea,
        key: &str,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
        source_generation: u64,
    ) -> Result<(), String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        let key = key.to_string();
        let old_value = self.with_map_read(area, &partition, |map| map.get(&key).cloned());
        if old_value.is_none() {
            return Ok(());
        }
        self.with_map_mut(area, &partition, |map| {
            map.remove(&key);
        });
        if area == StorageArea::Local {
            self.maybe_persist_local();
        }
        let origin = match &partition {
            Partition::Tuple { origin, .. } => origin.clone(),
            _ => String::new(),
        };
        let change = StorageChange {
            area,
            key: Some(key),
            old_value,
            new_value: None,
            page_url: page_url.to_string(),
            origin,
        };
        Self::emit_storage_change(&partition, tab_id, source_generation, change);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn clear(
        &self,
        area: StorageArea,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
        source_generation: u64,
    ) -> Result<(), String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        let had_keys = self.with_map_read(area, &partition, |map| !map.is_empty());
        if !had_keys {
            return Ok(());
        }
        self.with_map_mut(area, &partition, |map| map.clear());
        if area == StorageArea::Local {
            self.maybe_persist_local();
        }
        let origin = match &partition {
            Partition::Tuple { origin, .. } => origin.clone(),
            _ => String::new(),
        };
        let change = StorageChange {
            area,
            key: None,
            old_value: None,
            new_value: None,
            page_url: page_url.to_string(),
            origin,
        };
        Self::emit_storage_change(&partition, tab_id, source_generation, change);
        Ok(())
    }

    fn maybe_persist_local(&self) {
        if self.persist_path.is_none() {
            return;
        }
        let global = Self::global();
        if std::ptr::eq(self, global.as_ref()) {
            global.schedule_persist_local();
        } else {
            let _ = self.save();
        }
    }

    pub fn length(
        &self,
        area: StorageArea,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
    ) -> Result<u32, String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        Ok(self.with_map_read(area, &partition, |map| map.len() as u32))
    }

    pub fn key_at(
        &self,
        area: StorageArea,
        index: u32,
        page_url: &str,
        tab_id: u64,
        opaque_document_id: u64,
    ) -> Result<Option<String>, String> {
        let partition = Self::partition(page_url, tab_id, opaque_document_id)?;
        Ok(self.with_map_read(area, &partition, |map| map.key_at(index as usize).cloned()))
    }
}

fn is_opaque_storage_origin(url: &Url) -> bool {
    url.scheme() == "data" || !url.origin().is_tuple()
}

pub fn bootstrap_from_profile(profile_dir: Option<&Path>) -> Arc<WebStorage> {
    match profile_dir {
        Some(dir) => {
            let path = WebStorage::storage_path(dir);
            Arc::new(
                WebStorage::load_from_path(&path)
                    .unwrap_or_else(|_| WebStorage::with_persist_path(path)),
            )
        }
        None => Arc::new(WebStorage::empty()),
    }
}

impl PersistedFile {
    fn read(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let file = File::open(path).map_err(|e| e.to_string())?;
        #[derive(Deserialize, Default)]
        struct RawFile {
            #[serde(default)]
            origins: HashMap<String, serde_json::Value>,
        }
        let raw: RawFile = match serde_json::from_reader(BufReader::new(file)) {
            Ok(f) => f,
            Err(err) => {
                crate::logging::warn(
                    "profile.storage",
                    "local_storage.json corrupt or unreadable",
                    &[
                        ("error_kind", "json".into()),
                        ("detail", err.to_string()),
                        ("hint", "reset or delete local_storage.json (F-081)".into()),
                    ],
                );
                return Ok(Self::default());
            }
        };
        let mut origins = HashMap::new();
        for (origin, value) in raw.origins {
            if let Ok(p) = serde_json::from_value::<PersistedOrigin>(value.clone()) {
                origins.insert(origin, p);
            } else if let Ok(legacy) = serde_json::from_value::<BTreeMap<String, String>>(value) {
                let ordered = OrderedStore::from_legacy_map(legacy);
                origins.insert(origin, ordered.to_persisted());
            }
        }
        Ok(Self { origins })
    }

    fn write_atomic(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
        let json = serde_json::to_vec_pretty(self).map_err(|e| format!("json encode: {e}"))?;
        {
            let mut file = File::create(&tmp).map_err(|e| e.to_string())?;
            file.write_all(&json).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent()
            && let Ok(dir) = File::open(parent)
        {
            let _ = dir.sync_all();
        }
        Ok(())
    }
}

pub fn parse_area(raw: &str) -> Result<StorageArea, String> {
    match raw {
        "local" => Ok(StorageArea::Local),
        "session" => Ok(StorageArea::Session),
        other => Err(format!("unknown storage area: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> &str {
        s
    }

    #[test]
    fn local_storage_tuple_origin_survives_navigation_same_origin() {
        let store = WebStorage::empty();
        store
            .set_item(
                StorageArea::Local,
                "k",
                "v",
                url("http://example.test/a"),
                1,
                1,
                1,
            )
            .expect("set");
        let got = store
            .get_item(StorageArea::Local, "k", url("http://example.test/b"), 1, 2)
            .expect("get");
        assert_eq!(got.as_deref(), Some("v"));
    }

    #[test]
    fn session_storage_survives_same_tab_navigation() {
        let store = WebStorage::empty();
        store
            .set_item(
                StorageArea::Session,
                "s",
                "1",
                url("http://example.test/"),
                3,
                1,
                1,
            )
            .expect("set");
        let got = store
            .get_item(
                StorageArea::Session,
                "s",
                url("http://example.test/page"),
                3,
                9,
            )
            .expect("get");
        assert_eq!(got.as_deref(), Some("1"));
    }

    #[test]
    fn session_storage_isolated_between_tabs() {
        let store = WebStorage::empty();
        store
            .set_item(
                StorageArea::Session,
                "t",
                "a",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        let other = store
            .get_item(StorageArea::Session, "t", url("http://example.test/"), 2, 1)
            .expect("get");
        assert!(other.is_none());
    }

    #[test]
    fn opaque_data_origin_is_per_opaque_document_id() {
        let store = WebStorage::empty();
        let doc_a = allocate_opaque_document_id();
        let doc_b = allocate_opaque_document_id();
        store
            .set_item(StorageArea::Local, "x", "1", "data:,hi", 1, doc_a, 1)
            .expect("set");
        let other = store
            .get_item(StorageArea::Local, "x", "data:,hi", 1, doc_b)
            .expect("get");
        assert!(other.is_none());
    }

    #[test]
    fn quota_exceeded_error_when_over_limit() {
        let store = WebStorage::empty();
        let big = "x".repeat(QUOTA_BYTES);
        let err = store
            .set_item(
                StorageArea::Local,
                "big",
                &big,
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .unwrap_err();
        assert_eq!(err, "QuotaExceededError");
    }

    #[test]
    fn persist_round_trips_through_json() {
        let path =
            std::env::temp_dir().join(format!("glide-storage-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = WebStorage::with_persist_path(path.clone());
        store
            .set_item(
                StorageArea::Local,
                "persisted",
                "yes",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        store.flush_persist();
        let loaded = WebStorage::load_from_path(&path).expect("load");
        let got = loaded
            .get_item(
                StorageArea::Local,
                "persisted",
                url("http://example.test/"),
                1,
                1,
            )
            .expect("get");
        assert_eq!(got.as_deref(), Some("yes"));
    }

    #[test]
    fn remove_and_clear_update_values() {
        let store = WebStorage::empty();
        store
            .set_item(
                StorageArea::Local,
                "a",
                "1",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        store
            .remove_item(
                StorageArea::Local,
                "a",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("remove");
        assert!(
            store
                .get_item(StorageArea::Local, "a", url("http://example.test/"), 1, 1,)
                .expect("get")
                .is_none()
        );
        store
            .set_item(
                StorageArea::Local,
                "b",
                "2",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        store
            .clear(StorageArea::Local, url("http://example.test/"), 1, 1, 1)
            .expect("clear");
        assert_eq!(
            store
                .length(StorageArea::Local, url("http://example.test/"), 1, 1)
                .expect("len"),
            0
        );
    }

    #[test]
    fn key_index_matches_insertion_order() {
        let store = WebStorage::empty();
        store
            .set_item(
                StorageArea::Local,
                "b",
                "2",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        store
            .set_item(
                StorageArea::Local,
                "a",
                "1",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        let first = store
            .key_at(StorageArea::Local, 0, url("http://example.test/"), 1, 1)
            .expect("key")
            .expect("some");
        assert_eq!(first, "b");
    }

    #[test]
    fn set_item_unchanged_is_no_op() {
        let store = WebStorage::empty();
        store
            .set_item(
                StorageArea::Local,
                "k",
                "v",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("set");
        store
            .set_item(
                StorageArea::Local,
                "k",
                "v",
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("noop");
        assert_eq!(
            store
                .length(StorageArea::Local, url("http://example.test/"), 1, 1)
                .expect("len"),
            1
        );
    }

    #[test]
    fn quota_counts_utf16_code_units() {
        let store = WebStorage::empty();
        assert_eq!(WebStorage::utf16_len("é"), 1);
        assert_eq!(WebStorage::utf16_len("😀"), 2);
        let key = "k";
        let value = "😀".repeat(QUOTA_BYTES / 2 + 1);
        let err = store
            .set_item(
                StorageArea::Local,
                key,
                &value,
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .unwrap_err();
        assert_eq!(err, "QuotaExceededError");
        let accent = "é";
        store
            .set_item(
                StorageArea::Local,
                accent,
                accent,
                url("http://example.test/"),
                1,
                1,
                1,
            )
            .expect("accent fits");
    }

    #[test]
    fn persist_preserves_insertion_order_on_disk() {
        let path =
            std::env::temp_dir().join(format!("glide-storage-order-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let store = WebStorage::with_persist_path(path.clone());
        for key in ["z", "a", "m"] {
            store
                .set_item(
                    StorageArea::Local,
                    key,
                    key,
                    url("http://example.test/"),
                    1,
                    1,
                    1,
                )
                .expect("set");
        }
        store.flush_persist();
        let raw = std::fs::read_to_string(&path).expect("read");
        assert!(raw.find("\"z\"").expect("z") < raw.find("\"a\"").expect("a"));
        assert!(raw.find("\"a\"").expect("a") < raw.find("\"m\"").expect("m"));
        let loaded = WebStorage::load_from_path(&path).expect("load");
        assert_eq!(
            loaded
                .key_at(StorageArea::Local, 0, url("http://example.test/"), 1, 1)
                .expect("key")
                .as_deref(),
            Some("z")
        );
    }

    #[test]
    fn tuple_origin_persists_through_reinit_with_explicit_profile_dir() {
        let _guard = WebStorage::lock_global_for_test();
        let profile_dir =
            std::env::temp_dir().join(format!("glide-storage-profile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile_dir);
        std::fs::create_dir_all(&profile_dir).expect("mkdir");
        WebStorage::reinit_from_profile(Some(&profile_dir));
        let store = WebStorage::global();
        store
            .set_item(
                StorageArea::Local,
                "persist",
                "yes",
                url("http://127.0.0.1/"),
                1,
                1,
                1,
            )
            .expect("set");
        store.flush_persist();
        WebStorage::reinit_from_profile(Some(&profile_dir));
        let reloaded = WebStorage::global();
        let got = reloaded
            .get_item(
                StorageArea::Local,
                "persist",
                url("http://127.0.0.1/"),
                1,
                1,
            )
            .expect("get");
        assert_eq!(got.as_deref(), Some("yes"));
        WebStorage::reset_global();
    }
}
