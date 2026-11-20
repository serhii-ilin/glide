//! Glide HTML provider (scripting flag, JS event tap on iframe documents).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use blitz_dom::{BaseDocument, DocGuard, DocGuardMut, Document, DocumentConfig, DocumentMutator, HtmlParserProvider, NodeId};

use crate::js::JsTap;
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
        let inner = parse::document_from_html(
            html,
            config,
            self.scripting_enabled.load(Ordering::Relaxed),
        );
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
