---
slug: "page-navigation"
kind: "feature"
title: "Page-initiated navigation: forms and location"
status: "done"
aliases:
  [
    "F-072"
  ]
requirements:
  [
    "page-scripts-submit-forms-requestsubmit"
  ]
gherkin: "tests/features/page-navigation.feature"
tests:
  [
    "`js::integration::{request_submit_fires_submit_event_and_navigates,request_submit_rejects_foreign_submitter,form_submit_does_not_fire_submit_event,canceled_submit_event_prevents_navigation,location_assign_requests_navigation}`",
    "`js::apply::tests::{submit_form_rpc_navigates_with_the_entry_list,post_form_submit_navigates_with_post_method,post_form_entry_list_includes_named_submit_button,post_form_select_multiple_selected_options,post_form_formaction_targets_other_url,navigate_rpc_requests_the_target_url}`",
    "`http::tests::{post_form_document_navigation_is_urlencoded,post_navigation_includes_origin_and_content_type}`",
    "`nav_request::tests::navigation_options_to_document_request_post_urlencoded`",
    "`cookies::tests::{lax_cookie_omitted_on_cross_site_post_navigation,lax_cookie_sent_on_same_port_post_navigation}`",
    "`headless_js::{page_script_navigation_is_followed,headless_post_form_navigation_submits_urlencoded_body,headless_post_checkbox_checked_sends_on}`"
  ]
mcp: "`glide_navigate` (the headless session follows page-initiated navigations, including POST)"
manual: []
---

Page scripts can submit forms (`requestSubmit`, `submit`, `onsubmit`) and assign `location`, and the browser (including a headless load) follows the navigation, so JavaScript challenges and search forms work.

## Notes

Headless follows up to five page-initiated navigations. `submit_form` builds the entry list in Blitz (disabled omitted, submitter included, `formaction`/`formmethod`/`formenctype` on submit controls). POST uses `application/x-www-form-urlencoded` in `src/http.rs`. Back/forward history entries store URLs only; back replays GET without the POST body. The window iframe follows full `NavigationOptions` (method, headers, body) on the nav bus. DevTools Network records the document method. Cookies and Origin on POST use document-cookie.
