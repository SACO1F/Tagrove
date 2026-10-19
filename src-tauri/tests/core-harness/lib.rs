// Compile the production modules directly, without booting a Tauri WebView.
// This complements desktop integration testing; it does not replace it.
#![allow(dead_code)]
#[path = "../../src/ai.rs"]
mod ai;
#[path = "../../src/ai_review.rs"]
mod ai_review;
#[cfg(test)]
#[path = "../../src/ai_review_tests.rs"]
mod ai_review_tests;
#[path = "../../src/ai_worker.rs"]
mod ai_worker;
#[path = "../../src/annotation.rs"]
mod annotation;
#[cfg(test)]
#[path = "../../src/auto_tag_tests.rs"]
mod auto_tag_tests;
#[path = "../../src/auto_tags.rs"]
mod auto_tags;
#[path = "../../src/backup.rs"]
mod backup;
#[path = "../../src/cache.rs"]
mod cache;
#[path = "../../src/search.rs"]
mod search;
#[path = "../../src/pdf_index.rs"]
mod pdf_index;
#[path = "../../src/db.rs"]
mod db;
#[path = "../../src/fsops.rs"]
mod fsops;
#[path = "../../src/model.rs"]
mod model;
#[path = "../../src/tag_organize.rs"]
mod tag_organize;
#[path = "../../src/tag_aliases.rs"]
mod tag_aliases;
#[path = "../../src/office.rs"]
mod office;
#[path = "../../src/support.rs"]
mod support;
#[path = "../../src/window_geometry.rs"]
mod window_geometry;
#[cfg(test)]
#[path = "../../src/beta2_tests.rs"]
mod beta2_tests;

#[path = "../../src/transfer.rs"]
mod transfer;
#[cfg(test)]
#[path = "../../src/transfer_tests.rs"]
mod transfer_tests;

#[path = "../../src/path_repair.rs"]
mod path_repair;

#[path = "../../src/watch.rs"]
mod watch;

#[path = "../../src/text_document.rs"]
mod text_document;

#[path = "../../src/semantic.rs"]
mod semantic;
#[path = "../../src/natural_filter.rs"]
mod natural_filter;

#[path = "../../src/tag_groups.rs"]
mod tag_groups;

#[path = "../../src/reader_state.rs"]
mod reader_state;

#[path = "../../src/search_quality.rs"]
mod search_quality;

#[cfg(test)]
#[path = "../../src/performance_tests.rs"]
mod performance_tests;

#[path="../../src/capture.rs"]
mod capture;
#[path="../../src/capture_options.rs"]
mod capture_options;
#[path = "../../src/web_reading.rs"]
mod web_reading;

#[path = "../../src/image_capture.rs"]
mod image_capture;

#[path = "../../src/capture_browser.rs"]
mod capture_browser;
