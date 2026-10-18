> **Release 0.6.12:** verified on Windows 11 x64; equivalent Windows 10 acceptance is incomplete. The installer is unsigned. See the [release notes](release-v0.6.12.md) and [validation scope](validation-v0.6.12.md). Full bilingual product and extension instructions are in the [README](../README.md).

# Tagrove v0.6.12 — User guide

Tagrove is a Windows app for organizing local files with tags, notes, previews and search. It keeps references to your original files. Adding a file to the library does not move it.

## Start and personalize

Run the standalone EXE or install the setup package. Exit the old instance before launching the new version. On first launch you may configure AI tagging or skip it. The built-in quick guide follows the selected interface language.

Open **Preferences → Appearance** to select **English** or **简体中文**, a theme color, and light/dark/system appearance. Language and accent changes take effect immediately in the workspace and floating palette. File names, tags, aliases, groups, notes, document contents and existing audit text are not translated. Individual tag colors are independent of the app accent.

## Import and organize

Use **Add files** or drag files/folders into the workspace. Parent-folder tags can be generated automatically. Select a file to open Details; select it again or use the close icon to collapse Details. Ctrl-click selects multiple files. Ctrl+A toggles selection of current results; Ctrl+Z undoes supported operations.

Add tags in Details or from the floating tag button. The floating palette supports file tagging, individual colors, groups and tag order. Folder-origin tags are kept separate from regular tags and cannot be pinned as ordinary palette tags. AI suggestions need acceptance before becoming regular reusable tags.

## Search and preview

Search combines file information, indexed document text and optional semantic results. Local search remains available when semantic search is disabled or the service fails. Content indexing must be enabled for document body matches. Scanned images require other means of finding them, such as file names, tags and notes; this release does not add OCR.

Filter by tag, type, status, directory and other supported conditions. Save a named collection to reuse those conditions. Switch between masonry and list layouts. Ctrl+wheel over the file area changes masonry columns; over supported document previews it zooms content around the pointer.

Details contains **Content**, **Annotation** and **Info**. Images, PDF, Markdown, HTML and supported Office formats have local readers. Markdown editing changes the original file only when saved; review unsaved drafts before closing. Notes save automatically to the local annotation library. Office previews prioritize readable content and do not reproduce every layout feature.

## Optional AI and semantic search

AI tagging and semantic search are separate settings. Semantic search needs an Embeddings-compatible vector service and an embedding model, rather than a chat model. Review the service URL, model and key, and test the connection before confirming generation. Remote requests may incur charges. Review the listed file scope and text samples before sending them; disabling semantic search does not disable local search.

## Back up, upgrade and hand over

1. Before upgrading, export an annotation backup in Preferences and keep your important original files backed up separately.
2. Exit Tagrove, then run/install the new version. Keep existing user data when an installer asks. Verify file counts, tags, notes, favorites, aliases, groups and saved collections.
3. For another computer, export a **transfer package**, which includes original files and annotations. An annotation backup alone does not include original files.
4. On the receiving computer, import the package into a writable folder. Check the preview, then confirm. Files are organized under their original parent-folder names; name collisions are handled without silently overwriting existing originals.
5. Verify file counts, folders, notes and tags on the receiving machine. Keep the package until the handover has been checked.

## Troubleshooting and limits

If a file moves, use path repair or relink it after checking the candidate. Error messages in English include an explanation and error code when available; original OS/service diagnostics remain verbatim. Share diagnostics only after reviewing them.

The system file picker follows Windows language. Existing audit text and model responses retain their original language. This local delivery has automated and isolated desktop checks; real installer upgrades, two-computer handovers, multiple monitors/DPI settings and long-running use still require the acceptance checks supplied with the release.


## Date filters and theme names

Date filters accept strict `YYYY-MM-DD` values or selection from the calendar button inside the field. The calendar's months, weekdays and actions follow the app language. Choose the month and year, select a day, use Today or Clear date. Alt+Down opens it; arrow keys move through days, Enter selects, and Escape closes it. Invalid dates do not replace an existing valid filter. The end date includes the full selected local day, including daylight-saving changes.

Accent names now use Ultramarine (default), Lotus Leaf, Bellflower, Begonia, Golden Leaf and Blue Gray. Hover or focus a swatch for its name, color preview and HEX. Names take inspiration from Chinese traditional colors; the existing theme color values and custom tag colors are unchanged.

The current local delivery is `releases/v0.6.12/`; the executable and extension versions are 0.6.12. For the full current Chinese guide, see [使用说明](user-manual-v0.6.12.zh-CN.md).

The 12 accent choices also include Jade Teal, Ochre, Ink Blue, Vermilion, Moss Green and Cocoa Brown. Controls use the same tooltip appearance as Favorites, including the floating palette.


## Reading state

Reading positions, zoom and supported modes are restored per file after switching, closing the inspector or a normal restart. This includes PDF page position, image zoom and canvas scroll, workbook sheet and scroll, Word/plain-text scroll, HTML mode and frame position, Markdown mode and position, and presentation content page. Search targets take precedence. A changed source revision invalidates the old position.

Only numeric coordinates, page indices, modes and zoom metadata are stored locally, capped at 500 file records. Document text and PDF passwords are not included; this is not transferred in a handoff package. Unsaved Markdown drafts remain available while this process runs and return in editing mode. External edits are not overwritten on conflict. Closing still requires saving or explicitly discarding drafts; draft recovery after a crash is not included. Reading-state write failures show a retry notice and do not block reading.


## Search evidence

Active searches show filename, tag, alias, note, body or semantic evidence. Hover for details. Clearing the query hides evidence and excerpts. Exact aliases rank above filename substring matches; exact filenames retain priority.


## Library maintenance and recovery

Diagnostics now include database/index/preview sizes, task states, saved reading-state count and pending recovery status. Reports omit document names, paths, text, tag names, endpoints and keys; exporting is a local, explicit action.

Backups no longer overwrite existing files: choose a new filename. A metadata backup does not contain originals; use a transfer package for another computer. Restore keeps a protective backup first. Older backups can save reading state immediately after restore. On interrupted restore recovery, upload configurations are disabled, pending AI jobs cancelled, folder watchers paused and derived indexes cleared. Review the library, then explicitly re-enable watchers, configure services and rebuild indexes as needed.

Fuse.js remains a planning candidate and is not installed or integrated.

## Browser capture: links, excerpts and images

Use desktop and Chrome extension version 0.6.12. Unzip the extension into its existing fixed folder, then Reload at chrome://extensions with Developer mode enabled. Keep the Chrome profile and extension data to preserve pending requests. In desktop Preferences, connect the Chrome extension; repair the connection after moving the executable. The extension interface is currently Chinese.

Right-click an image, choose Save image to Tagrove, review its title, source page and image URL, then explicitly submit. Alternatively choose image mode in the popup and enter an image URL. Only public HTTP/HTTPS PNG, JPEG, WebP, GIF and BMP images are supported, up to 50 MiB and 100 million pixels. The downloaded original bytes are saved locally with page URL, original/final image URL, capture time, dimensions, MIME and SHA256. Normal thumbnails and image preview are available; add tags, notes and favorites as usual.

The optional preview button makes an anonymous browser request only when clicked. CORS may block this preview while desktop download still works. Desktop download does not carry browser cookies, authorization or proxy settings. A 403 retry may send an origin-only Referer when safe. Login-only, private-network, SVG, blob/data images, screenshots and whole-site collection are not supported. For restricted images, save the corresponding original in the browser, then choose it from the failed queue item to preserve its source and tags. Untagged captures queue AI recognition when desktop automatic tagging is enabled; suggestions require acceptance. Capture does not create source-folder tags.

Links save a URL and, when readable, a bounded plain-text snapshot; excerpts store plain text, up to 20,000 characters. Titles allow 300 characters. Independent drafts are limited to 20 for 24 hours. Text input and textarea selections are supported, excluding passwords; protected pages and cross-origin frames may restrict reading.

When desktop is offline, the current Chrome profile queues URLs and metadata, not image bytes. Start desktop and retry. A temporary image URL may expire before retry. The queue holds up to 200 items and does not evict unfinished work. Stable request IDs prevent duplicate retries; matching image content reuses the first record without replacing its source, title, tags, notes or favorite status. Disconnecting or removing the local connection does not delete library items or browser queue data.

## Web reading and organization in v0.6.12

Select a web item and use Read, Edit or Original capture in its content panel. Editing changes only a reading copy of the title and plain-text body. Original title, excerpt, source URL, capture time and receipt deduplication remain unchanged. Save explicitly or press Ctrl+S; limits are 300 title characters and 20,000 body characters. Switching files preserves unsaved drafts in this process. Exit prompts before discarding them; crash recovery is not provided.

Concurrent changes reject a stale save and retain your draft. Reload explicitly discards the draft; Restore original capture also removes the saved copy after confirmation. Saves and restores support session undo. Each save writes a new managed descriptor and updates SQLite transactionally; older descriptors are retained for undo. Do not manually clean the captures folder.

Ordinary search uses the current title and reading text. If body indexing is enabled, edits are reindexed; stale revisions cannot supply old snippets. The reader highlights active search terms and offers Next match. Content is plain text and does not execute scripts.

Filters now support an exact source website domain and capture type. Enter a domain without a protocol, port or path, then press Enter or blur to apply. Subdomains are distinct. Save the conditions as a collection. Select matching results with Ctrl+A and use batch annotation to add common tags. Tag selection accepts canonical names and aliases, excluding tags already present on every selected item.

## Upgrade and handoff in v0.6.12

Export a metadata backup and back up original files before upgrading. Schema 7 prevents older apps from opening the upgraded library. Schema 6 receives a protective snapshot at backups/before-preview-image-schema-v6.sqlite. The new app can read older backups and packages.

Captured-image packages use format 7 and require v0.6.12 on the receiving computer. Reading-copy packages use format 6, link/excerpt packages format 4, ordinary local packages format 3. Backups contain metadata, not originals; use transfer packages for another computer.

The extension version is 0.6.12 with unchanged protocol and permissions. Image limits and public-network restrictions remain as described above. Clean installation, enterprise policies, physical two-computer handoff and prolonged use still require manual acceptance.


## Original layout and live websites in v0.6.12

Open an Office item, then choose Original layout. Installed engines are detected per document type, preferring Microsoft Office, WPS Office, then LibreOffice. Choose an engine explicitly to save a preference. This conservative adapter refuses to reuse a running application of the same type: close that application or choose another engine. It does not change file associations or close user documents. Install enhanced preview component opens the official LibreOffice download page; installation is manual. Detect again afterwards.

DOC/DOCX, XLS/XLSX and PPT/PPTX share read-only PDF layout previews. Legacy content mode converts to a temporary modern format and uses the existing paragraph, sheet or slide-content reader. Excel grid mode and print-layout mode have different purposes. Source and converted files are each limited to 64 MiB; existing content parsing limits remain. Conversion times out after 90 seconds. Activation, protected view, passwords or engine errors may require checking Office, another engine or opening the original externally. Preview does not edit sources or play animations. WPS, LibreOffice and complex SmartArt fidelity still need acceptance on the corresponding machines.

PDF pages now provide selectable existing text. Scans without a text layer still require OCR, which is not included.

Opening a web item defaults to Live website and loads its source into a separate WebView2 area. Back, forward, reload, address display and external opening are provided. The profile does not share Chrome cookies; native library APIs, downloads and new windows are unavailable to websites. Returning to Reading copy or switching files destroys the live view. This is online browsing, not offline archiving.

GIF/BMP capture and local import are supported; GIF preview shows its first frame. The image limit is 50 MiB and 100 million pixels. Private-network, authenticated, SVG and blob/data capture remain unsupported. Update the extension to v0.6.12 as well.


## Capture tags and live-preview fixes in v0.6.12

Before submitting a browser capture, search and select existing regular tags. Folder-origin tags and unaccepted AI suggestions are excluded. Select at most 50 tags; search returns up to 500 matching results. Cached tags remain available offline. Deleted tags need correction using Re-edit in the unfinished queue. Re-edit creates a new request ID; unchanged retries keep their ID.

When automatic AI tagging is enabled, new links, excerpts and images queue recognition only if no tags were selected. Links use a readable snapshot (up to 12,000 characters) or title and URL; excerpts use selected text; images use image content. Captured content is sent to the configured model service and may incur charges. Existing manual tags are preserved. AI suggestions need desktop acceptance before entering the reusable tag pool. Disabled AI sends no recognition requests. Duplicate captures can add selected tags without re-queuing AI. Existing records are not automatically backfilled.

Live website now displays in both Details and full preview. Permanent file lists no longer hide it; each surface owns its view session so stale cleanup cannot close the active view. This was visually verified using the supplied regex tutorial. Local HTML remains static/source preview; a web capture with a source URL is needed for Live website. Login, network and website compatibility limitations remain.

Update both desktop and extension to 0.6.12, reload the extension in its existing folder and repair its local connection after moving the EXE. The extension ID, permissions and protocol remain unchanged. Schema remains 7.


## v0.6.12 popup and preview controls

Context-menu capture now opens the native Chrome toolbar action popup rather than a new tab. Pin Tagrove for easy access. Chrome 127+ supports direct opening; if blocked, the draft remains and the action badge prompts a manual click. Popup content uses large rounded corners; Chrome owns the outer frame.

Live website and Original layout are first in their respective toolbars. They share the Read/Edit capsule sizing and interaction, with small icons. Office initially opens Original layout; engine selection and Detect engines appear in Original layout only. Legacy Office keeps its available-engine path.

First reads default to Fit page for PDFs, images, Office layouts and flowing content. Existing saved zoom remains unchanged. Flowing text reflows at the reading width; it does not shrink the entire long document to one screen. Online websites keep their own responsive layout.

Excel PDF conversion through Microsoft/WPS enables print gridlines in an isolated read-only workbook without saving source changes. LibreOffice retains source print settings and still needs dedicated acceptance. A new cache version prevents stale old PDFs. Engine detection is visually quiet. Layout conversion uses a centered stationary page placeholder and subtle sweep, without fake percentages or extra dependencies, respecting reduced motion.


## v0.6.12 default preview and reading-copy modes

Web items now default to Live website and connect to the source; Office defaults to Original layout. Other formats keep their own readers and initial Fit page. Engine detection has no visible loading animation; an existing preview remains during re-detection. Actual PDF layout generation centers the frame and status together within the content area. Saving a reading copy preserves its current surface instead of jumping online.

Read displays the current local text copy (or original title/excerpt when none exists). Edit changes only its title/text after explicit saving, not the source website. Original capture is a read-only comparison with the initial title/excerpt/source and does not undo edits. Restore original capture is a separate confirmed action that removes the saved copy. A link may originally contain only a title and URL; the extension's text snapshot is stored as a reading copy and is not necessarily part of the original excerpt. Neither is a complete offline webpage.


## v0.6.12 suggestion boundaries and reading spacing

Folder-origin tags are excluded from the AI request's candidate pool and from its 1,000-tag limit. Folder IDs/names returned by the model are ignored, with a second check before writing suggestions.

Browser links, excerpts and images saved with selected tags do not create automatic AI jobs. Untagged captures still follow the AI switch. Adding selected tags to a duplicate capture cancels queued/running recognition result acceptance; already-sent requests may still be billed. Completed suggestions are not retroactively removed. Explicit manual recognition remains available.

Reading-copy controls keep a 20px safe gap below the separator before the webpage-link kind and source content, across Read, Edit and Original capture.

## Capture failure diagnostics (v0.6.12)

The Chinese Chrome extension shows the failure stage, error code, sanitized underlying reason and recovery advice in its popup and queue. Technical details include HTTP status where available, time, request ID, capture type, attempt count, elapsed time and known versions. Copy diagnostic report to share with the developer; reports omit captured text, titles, full URLs and local paths and redact common credential fields. Review remaining text before sharing. Nothing is uploaded automatically. Failed queued items remain local; validation or browser storage failures may not have been queued. Legacy records explicitly indicate missing original details. Successful retries remove stale errors.

## HTTP 403 image recovery (v0.6.12)

After a 403, the desktop retries once with an origin-only Referer when safe. It does not send a page path or query, browser cookies or credentials, and omits Referer on HTTPS-to-HTTP downgrade. Public destination and redirect validation remains enabled. This cannot guarantee access to restricted sites. If it fails, use the Chinese extension queue: Open source page → download the corresponding original using the browser → Select downloaded original. The desktop file picker lets the user choose a local image; validated bytes are copied into the library with the original task source, title and tags. Cancel leaves the queue item intact. Existing 50 MiB and 100 million pixel limits apply. The user must verify the selected file corresponds to the remote image. No new Chrome permissions are added. Update both desktop and extension and repair the native connection after moving the EXE.


## Interface text size

Preferences → Font size offers five sizes, from Smaller to Largest (12–16, default 13). Changes are saved and synchronized with floating tags. Document zoom remains a separate control.


## Floating palette update — 2026-10-18

Three additional accents: Vermilion, Moss green and Cocoa brown. Drag a tag by its nine-dot handle onto a category folder heading; release when highlighted to reclassify it. Empty and collapsed categories accept drops. Drop on Uncategorized to remove category membership; Escape cancels. Dragging the tag body onto a file still applies the tag to that file.


## Settings disclosures and tag-drop motion — 2026-10-18

Click a settings group heading or arrow to expand it. Several groups can stay open; collapsing preserves unsaved form input. Appearance → 3D tag-drop feedback controls the gentle tilt while dragging tags across file cards. It defaults to on and remembers your choice. Reduced-motion preferences disable the tilt; tagging still works when the effect is off.

The 2026-10-19 branding delivery uses a transparent Tagrove logo across the desktop app and Chrome extension. Ultramarine is the default accent; existing saved color preferences are preserved. Source updates and published GitHub Releases are separate; check the release version before downloading.
