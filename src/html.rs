//! Glide HTML provider (scripting flag, JS event tap on iframe documents).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use blitz_dom::{
    BaseDocument, DocGuard, DocGuardMut, Document, DocumentConfig, DocumentMutator,
    HtmlParserProvider, NodeId,
};

use crate::js::{JsHost, JsTap};
use crate::parse;

#[derive(Clone)]
pub struct GlideHtmlProvider {
    pub scripting_enabled: Arc<AtomicBool>,
    pub js_tap: JsTap,
}

impl Default for GlideHtmlProvider {
    fn default() -> Self {
        Self {
            scripting_enabled: Arc::new(AtomicBool::new(true)),
            js_tap: JsTap::new_disabled(),
        }
    }
}

/// Apply a runtime JS on/off choice to the shared HTML provider and JS host.
pub fn set_js_runtime_mode(
    provider: &GlideHtmlProvider,
    host: &mut JsHost,
    chrome: &BaseDocument,
    on: bool,
) {
    provider.scripting_enabled.store(on, Ordering::Relaxed);
    provider.js_tap.set_enabled(on);
    host.set_enabled(on, chrome);
}

impl HtmlParserProvider for GlideHtmlProvider {
    fn parse_inner_html<'m, 'doc>(
        &self,
        mutr: &'m mut DocumentMutator<'doc>,
        element_id: NodeId,
        html: &str,
    ) {
        parse::parse_inner_html_into_mutator(
            mutr,
            element_id,
            html,
            self.scripting_enabled.load(Ordering::Relaxed),
        );
    }

    fn parse_document(&self, html: &str, config: DocumentConfig) -> Box<dyn Document> {
        let inner =
            parse::document_from_html(html, config, self.scripting_enabled.load(Ordering::Relaxed));
        if self.js_tap.enabled() {
            Box::new(JsPageDocument::new(inner, self.js_tap.clone()))
        } else {
            Box::new(HtmlPageDocument(inner))
        }
    }
}

/// Page document parsed with Glide's HTML driver (same shape as `blitz_html::HtmlDocument`).
pub struct HtmlPageDocument(pub BaseDocument);

impl Document for HtmlPageDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::Ref(&self.0)
    }
    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::Ref(&mut self.0)
    }
}

pub struct JsPageDocument {
    inner: BaseDocument,
    tap: JsTap,
}

impl JsPageDocument {
    fn new(inner: BaseDocument, tap: JsTap) -> Self {
        Self { inner, tap }
    }
}

impl Document for JsPageDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::Ref(&self.inner)
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::Ref(&mut self.inner)
    }

    fn handle_ui_event(&mut self, event: blitz_traits::events::UiEvent) {
        self.tap.record_ui_event(&event);
        self.inner.handle_ui_event(event);
    }

    fn id(&self) -> usize {
        self.inner.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use crate::parse;

    #[test]
    fn set_js_runtime_mode_updates_shared_flags() {
        let tap = JsTap::new_enabled();
        let provider = GlideHtmlProvider {
            scripting_enabled: Arc::new(AtomicBool::new(true)),
            js_tap: tap.clone(),
        };
        let mut host = JsHost::new(true, tap.clone(), None);
        let chrome = parse::document_from_html(
            "<!DOCTYPE html><html><body></body></html>",
            DocumentConfig::default(),
            false,
        );
        assert!(provider.js_tap.enabled());

        set_js_runtime_mode(&provider, &mut host, &chrome, false);
        assert!(!provider.scripting_enabled.load(Ordering::Relaxed));
        assert!(!provider.js_tap.enabled());
        assert!(!host.is_enabled());

        set_js_runtime_mode(&provider, &mut host, &chrome, true);
        assert!(provider.scripting_enabled.load(Ordering::Relaxed));
        assert!(provider.js_tap.enabled());
        assert!(host.is_enabled());
    }
}
