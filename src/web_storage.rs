//! Origin-scoped Web Storage (`localStorage` / `sessionStorage`) with profile persistence.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use url::Url;

const STORAGE_FILE: &str = "local_storage.json";
pub const QUOTA_BYTES: usize = 5 * 1024 * 1024;

static GLOBAL: RwLock<Option<Arc<WebStorage>>> = RwLock::new(None);

#[cfg(test)]
static GLOBAL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageArea {
    Local,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Partition {
    Tuple { origin: String, tab_id: u64 },
    Opaque { tab_id: u64, doc_generation: u64 },
}

#[derive(Debug, Clone)]
pub struct StorageChange {
    pub area: StorageArea,
    pub key: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub page_url: String,
    pub origin: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedFile {
    #[serde(default)]
    origins: HashMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Default)]
struct OrderedStore {
    order: Vec<String>,
    values: HashMap<String, String>,
}

impl OrderedStore {
    fn from_persisted(map: BTreeMap<String, String>) -> Self {
        let order: Vec<String> = map.keys().cloned().collect();
        let values = map.into_iter().collect();
        Self { order, values }
    }

    fn to_persisted(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for key in &self.order {
            if let Some(value) = self.values.get(key) {
                out.insert(key.clone(), value.clone());
            }
        }
        out
    }

    fn len(&self) -> usize {
        self.values.len()
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
}

impl WebStorage {
    pub fn empty() -> Self {
        Self {
            local_tuple: RwLock::new(HashMap::new()),
            local_opaque: RwLock::new(HashMap::new()),
            session_tuple: RwLock::new(HashMap::new()),
            session_opaque: RwLock::new(HashMap::new()),
            persist_path: None,
        }
    }

    pub fn with_persist_path(path: PathBuf) -> Self {
        Self {
            local_tuple: RwLock::new(HashMap::new()),
            local_opaque: RwLock::new(HashMap::new()),
            session_tuple: RwLock::new(HashMap::new()),
            session_opaque: RwLock::new(HashMap::new()),
            persist_path: Some(path),
        }
    }

    pub fn storage_path(profile_dir: &Path) -> PathBuf {
        profile_dir.join(STORAGE_FILE)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let file = PersistedFile::read(path)?;
        let mut tuple = HashMap::new();
        for (origin, map) in file.origins {
            tuple.insert(origin, OrderedStore::from_persisted(map));
        }
        Ok(Self {
            local_tuple: RwLock::new(tuple),
            local_opaque: RwLock::new(HashMap::new()),
            session_tuple: RwLock::new(HashMap::new()),
            session_opaque: RwLock::new(HashMap::new()),
            persist_path: Some(path.to_path_buf()),
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
        snapshot.write_atomic(path)
    }

    fn maybe_persist_local(&self) {
        if self.persist_path.is_some() {
            let _ = self.save();
        }
    }

    fn partition(page_url: &str, tab_id: u64, doc_generation: u64) -> Result<Partition, String> {
        let url = Url::parse(page_url).map_err(|e| e.to_string())?;
        if is_opaque_storage_origin(&url) {
            return Ok(Partition::Opaque {
                tab_id,
                doc_generation,
            });
        }
        Ok(Partition::Tuple {
            origin: url.origin().ascii_serialization(),
            tab_id,
        })
    }

    fn map_size(map: &OrderedStore) -> usize {
        map.iter_pairs().map(|(k, v)| k.len() + v.len()).sum()
    }

    fn check_quota(map: &OrderedStore, key: &str, new_value: &str) -> Result<(), String> {
        let old_len = map.get(key).map(|v| key.len() + v.len()).unwrap_or(0);
        let new_len = key.len() + new_value.len();
        let total = Self::map_size(map) + new_len.saturating_sub(old_len);
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
        doc_generation: u64,
    ) -> Result<serde_json::Value, String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
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
            (StorageArea::Local, Partition::Opaque { doc_generation, .. }) => {
                let mut guard = self.local_opaque.write().unwrap_or_else(|p| p.into_inner());
                f(guard.entry(*doc_generation).or_default())
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
                    doc_generation,
                },
            ) => {
                let mut guard = self
                    .session_opaque
                    .write()
                    .unwrap_or_else(|p| p.into_inner());
                f(guard.entry((*tab_id, *doc_generation)).or_default())
            }
        }
    }

    fn with_map_read<R>(
        &self,
        area: StorageArea,
        partition: &Partition,
        f: impl FnOnce(&OrderedStore) -> R,
    ) -> R {
        match (area, partition) {
            (StorageArea::Local, Partition::Tuple { origin, .. }) => {
                let guard = self.local_tuple.read().unwrap_or_else(|p| p.into_inner());
                let map = guard.get(origin).cloned().unwrap_or_default();
                f(&map)
            }
            (StorageArea::Local, Partition::Opaque { doc_generation, .. }) => {
                let guard = self.local_opaque.read().unwrap_or_else(|p| p.into_inner());
                let map = guard.get(doc_generation).cloned().unwrap_or_default();
                f(&map)
            }
            (StorageArea::Session, Partition::Tuple { origin, tab_id }) => {
                let guard = self.session_tuple.read().unwrap_or_else(|p| p.into_inner());
                let map = guard
                    .get(&(*tab_id, origin.clone()))
                    .cloned()
                    .unwrap_or_default();
                f(&map)
            }
            (
                StorageArea::Session,
                Partition::Opaque {
                    tab_id,
                    doc_generation,
                },
            ) => {
                let guard = self
                    .session_opaque
                    .read()
                    .unwrap_or_else(|p| p.into_inner());
                let map = guard
                    .get(&(*tab_id, *doc_generation))
                    .cloned()
                    .unwrap_or_default();
                f(&map)
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
        doc_generation: u64,
    ) -> Result<Option<String>, String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
        Ok(self.with_map_read(area, &partition, |map| map.get(key).cloned()))
    }

    #[allow(clippy::too_many_arguments)] // mirrors Web Storage setter inputs from DOM RPC
    pub fn set_item(
        &self,
        area: StorageArea,
        key: &str,
        value: &str,
        page_url: &str,
        tab_id: u64,
        doc_generation: u64,
        source_generation: u64,
    ) -> Result<(), String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
        let key = key.to_string();
        let value = value.to_string();
        let old_value = self.with_map_read(area, &partition, |map| map.get(&key).cloned());
        self.with_map_mut(area, &partition, |map| {
            Self::check_quota(map, &key, &value)?;
            map.insert(key.clone(), value.clone());
            Ok::<(), String>(())
        })?;
        if area == StorageArea::Local {
            self.maybe_persist_local();
        }
        if let Partition::Tuple { origin, .. } = &partition {
            let change = StorageChange {
                area,
                key,
                old_value,
                new_value: Some(value),
                page_url: page_url.to_string(),
                origin: origin.clone(),
            };
            crate::js::storage_events::notify_other_listeners(origin, source_generation, &change);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_item(
        &self,
        area: StorageArea,
        key: &str,
        page_url: &str,
        tab_id: u64,
        doc_generation: u64,
        source_generation: u64,
    ) -> Result<(), String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
        let key = key.to_string();
        let old_value = self.with_map_read(area, &partition, |map| map.get(&key).cloned());
        self.with_map_mut(area, &partition, |map| {
            map.remove(&key);
        });
        if area == StorageArea::Local {
            self.maybe_persist_local();
        }
        if let (Partition::Tuple { origin, .. }, Some(_)) = (&partition, &old_value) {
            let change = StorageChange {
                area,
                key,
                old_value,
                new_value: None,
                page_url: page_url.to_string(),
                origin: origin.clone(),
            };
            crate::js::storage_events::notify_other_listeners(origin, source_generation, &change);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn clear(
        &self,
        area: StorageArea,
        page_url: &str,
        tab_id: u64,
        doc_generation: u64,
        source_generation: u64,
    ) -> Result<(), String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
        let prior: Vec<(String, String)> = self.with_map_read(area, &partition, |map| {
            map.iter_pairs()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        });
        self.with_map_mut(area, &partition, |map| map.clear());
        if area == StorageArea::Local {
            self.maybe_persist_local();
        }
        if let Partition::Tuple { origin, .. } = &partition {
            for (key, value) in prior {
                let change = StorageChange {
                    area,
                    key,
                    old_value: Some(value),
                    new_value: None,
                    page_url: page_url.to_string(),
                    origin: origin.clone(),
                };
                crate::js::storage_events::notify_other_listeners(
                    origin,
                    source_generation,
                    &change,
                );
            }
        }
        Ok(())
    }

    pub fn length(
        &self,
        area: StorageArea,
        page_url: &str,
        tab_id: u64,
        doc_generation: u64,
    ) -> Result<u32, String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
        Ok(self.with_map_read(area, &partition, |map| map.len() as u32))
    }

    pub fn key_at(
        &self,
        area: StorageArea,
        index: u32,
        page_url: &str,
        tab_id: u64,
        doc_generation: u64,
    ) -> Result<Option<String>, String> {
        let partition = Self::partition(page_url, tab_id, doc_generation)?;
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
        serde_json::from_reader(BufReader::new(file)).map_err(|e| e.to_string())
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
    fn opaque_data_origin_is_per_document_generation() {
        let store = WebStorage::empty();
        store
            .set_item(StorageArea::Local, "x", "1", "data:,hi", 1, 1, 1)
            .expect("set");
        let next = store
            .get_item(StorageArea::Local, "x", "data:,hi", 1, 2)
            .expect("get");
        assert!(next.is_none());
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
        store.save().expect("save");
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
}
