//! In-memory bookmark list mirrored to the profile on toggle.

use std::sync::{Arc, RwLock};

use blitz_dom::BaseDocument;

use crate::chrome::set_bookmark_star;
use crate::profile::{Bookmark, ProfileStore};

#[derive(Debug)]
pub struct BookmarksState {
    entries: Vec<Bookmark>,
    store: Arc<ProfileStore>,
    star_url: Option<String>,
    star_shown: Option<bool>,
}

impl BookmarksState {
    pub fn load(store: Arc<ProfileStore>) -> Self {
        Self {
            entries: store.load_bookmarks(),
            store,
            star_url: None,
            star_shown: None,
        }
    }

    pub fn entries(&self) -> &[Bookmark] {
        &self.entries
    }

    pub fn shared(entries: Arc<RwLock<Vec<Bookmark>>>) -> Arc<dyn Fn() -> String + Send + Sync> {
        Arc::new(move || {
            let guard = entries
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            crate::profile::bookmarks_page_html(&guard)
        })
    }

    pub fn is_bookmarked(&self, url: &str) -> bool {
        self.entries.iter().any(|b| b.url == url)
    }

    pub fn sync_star(&mut self, doc: &mut BaseDocument, url: &str) {
        let want = self.is_bookmarked(url);
        if self.star_url.as_deref() == Some(url) && self.star_shown == Some(want) {
            return;
        }
        set_bookmark_star(doc, want);
        self.star_url = Some(url.to_string());
        self.star_shown = Some(want);
    }

    pub fn toggle(
        &mut self,
        doc: &mut BaseDocument,
        url: &str,
        title: &str,
        now_unix: u64,
    ) -> bool {
        let bookmarked = if let Some(pos) = self.entries.iter().position(|b| b.url == url) {
            self.entries.remove(pos);
            false
        } else {
            self.entries.push(Bookmark {
                url: url.to_string(),
                title: title.to_string(),
                added_unix: now_unix,
            });
            true
        };
        let _ = self.store.save_bookmarks(&self.entries);
        set_bookmark_star(doc, bookmarked);
        self.star_url = Some(url.to_string());
        self.star_shown = Some(bookmarked);
        bookmarked
    }
}
