//! The main window's document tab strip: one framework `DocumentTabs` over the docked documents and the files still
//! opening, the tab menu, and the title a document's tab and canvas show.

use craft::ui::contract::overlay::Anchor;
use craft::ui::contract::{Bind, BodyCx, Entry, Remember as _, Trigger};
use craft::ui::menu::Menu;
use craft::ui::tabs::{DocumentTab, DocumentTabs, DocumentTabsEvent, DocumentTabsOutput};
use egui::{Pos2, Response};
use photocraft_doc::DocId;
use photocraft_engine::jobs::JobId;

use super::{PhotocraftApp, fmt_zoom, mode_label};
use crate::file_open::DropTo;
use serde_json::json;

/// A tab of the document strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum DocTab {
    /// An open document.
    Doc(DocId),
    /// A file still opening in the background (#210): its job.
    Opening(JobId),
}

/// The tab menu while it is open: the document it was asked for and where it hangs. It names the document, not its
/// place on the strip, so a reorder while the menu is open cannot point it at another tab.
#[derive(Clone, Debug, Default)]
struct TabMenu(Option<(DocId, Pos2)>);

/// The index of document `id` in the session's document list.
pub(crate) fn document_index(app: &PhotocraftApp, id: DocId) -> Option<usize> {
    app.session.documents().iter().position(|st| st.doc.id == id)
}

/// The body of the canvas panel's tabs region: the strip, what the user did on it, and the tab menu a right click
/// asked for.
pub(super) fn strip(app: &mut PhotocraftApp, ui: &mut egui::Ui, cx: &mut BodyCx<'_>) {
    let ctx = ui.ctx().clone();
    let out = tabs(app, ui);
    let response = out.response.clone();
    if let Some(menu) = apply(app, &ctx, out) {
        cx.keep::<TabMenu>().0 = Some(menu);
    }
    tab_menu(app, ui, cx, &response);
}

/// The tab strip: a tab per docked document, in document order, then one per file still opening. The tab of the
/// document the main canvas shows is selected, or the opening file whose progress is shown.
pub(super) fn tabs(app: &PhotocraftApp, ui: &mut egui::Ui) -> DocumentTabsOutput<DocTab> {
    let docs = app.session.documents();
    let mut tabs: Vec<DocumentTab<DocTab>> = docked_document_indices(app)
        .into_iter()
        .filter_map(|i| {
            let st = docs.get(i)?;
            let zoom = app.ui.views.get(i).map_or(100.0, |v| v.zoom * 100.0);
            let (name, detail) = tab_title_at(app, i, zoom);
            Some(DocumentTab::new(DocTab::Doc(st.doc.id), name).detail(detail).modified(st.is_dirty()))
        })
        .collect();
    tabs.extend(
        crate::jobs_ui::open_tabs(app)
            .into_iter()
            .map(|(job, name, fraction)| DocumentTab::new(DocTab::Opening(job), name).progress(fraction).close_label(tl!("Cancel opening"))),
    );
    let selected = app.jobs.focus.map(DocTab::Opening).or_else(|| main_canvas_document(app).and_then(|i| docs.get(i)).map(|st| DocTab::Doc(st.doc.id)));
    let mut strip =
        DocumentTabs::new(tl!("Documents"), &tabs).reorderable(true).tear_off(true).large(app.session.prefs().workspace.large_tabs).takes_files(true);
    if let Some(tab) = selected {
        strip = strip.selected(tab);
    }
    strip.show(ui)
}

/// Applies what the user did on the strip this pass. The strip reports only the input its own pass received, so a
/// second pass of a frame applies nothing twice. Returns the tab menu a right click asked for.
pub(super) fn apply(app: &mut PhotocraftApp, ctx: &egui::Context, out: DocumentTabsOutput<DocTab>) -> Option<(DocId, Pos2)> {
    let mut menu = None;
    // A Move-tool drag over a tab turns into a layer transfer to that document (`layer_transfer`).
    if let Some(DocTab::Doc(id)) = out.hovered
        && crate::layer_transfer::pointer_if_armed(app, ctx).is_some()
        && let Some(i) = document_index(app, id)
    {
        crate::layer_transfer::over_tab(app, ctx, i);
    }
    for event in out.events {
        match event {
            DocumentTabsEvent::Selected(tab) | DocumentTabsEvent::Picked(tab) | DocumentTabsEvent::Activated(tab) => show_tab(app, tab),
            DocumentTabsEvent::CloseRequested(DocTab::Doc(id)) => {
                // The guarded close: a document with unsaved changes gets the save prompt.
                let _ = crate::menus::invoke(app, ctx, "file.close", json!({"document": id.0}));
            }
            DocumentTabsEvent::CloseRequested(DocTab::Opening(job)) => {
                let _ = crate::jobs_ui::cancel(app, job);
            }
            DocumentTabsEvent::Reordered { documents, before, .. } => reorder(app, ctx, &documents, before),
            DocumentTabsEvent::TornOff { documents, at, .. } => {
                for tab in documents {
                    if let DocTab::Doc(id) = tab {
                        run(app, ctx, "window.arrange.floatInWindow", json!({"document": id.0, "position": [at.x, at.y]}));
                    }
                }
            }
            DocumentTabsEvent::FilesDropped { files, before, .. } => {
                // The files open before the tab they were dropped in front of, or after every document.
                let slot = match before {
                    Some(DocTab::Doc(id)) => document_index(app, id),
                    _ => None,
                }
                .unwrap_or(app.session.documents().len());
                app.open_dropped(ctx, files, DropTo::Slot(slot));
            }
            DocumentTabsEvent::ContextMenu(request) => {
                if let DocTab::Doc(id) = request.key {
                    menu = Some((id, request.at));
                }
            }
            _ => {}
        }
    }
    menu
}

/// A chosen tab: a document tab shows that document, an opening tab its progress.
fn show_tab(app: &mut PhotocraftApp, tab: DocTab) {
    match tab {
        DocTab::Doc(id) => {
            if let Some(i) = document_index(app, id) {
                app.session.set_active(i);
                app.jobs.focus = None;
            }
        }
        DocTab::Opening(job) => app.jobs.focus = Some(job),
    }
}

/// A dragged document tab now sits before `before` (last when `None` or an opening file's tab): `document.move` to
/// the matching place in the document list, floating documents keeping theirs.
fn reorder(app: &mut PhotocraftApp, ctx: &egui::Context, moved: &[DocTab], before: Option<DocTab>) {
    let Some(from) = moved.iter().find_map(|tab| match tab {
        DocTab::Doc(id) => document_index(app, *id),
        DocTab::Opening(_) => None,
    }) else {
        return;
    };
    let visible = docked_document_indices(app);
    let remaining: Vec<usize> = visible.iter().copied().filter(|&d| d != from).collect();
    let before = match before {
        Some(DocTab::Doc(id)) => document_index(app, id),
        _ => None,
    };
    let to = before.and_then(|b| remaining.iter().position(|&d| d == b)).unwrap_or(remaining.len());
    let target = move_target_after_filter(&visible, from, to, app.session.documents().len());
    if target != from {
        run(app, ctx, "document.move", json!({"document": from, "to": target}));
    }
}

/// Runs a tab command through the menus' guarded path; a refusal goes to the status line.
fn run(app: &mut PhotocraftApp, ctx: &egui::Context, id: &'static str, params: serde_json::Value) {
    if let Err(e) = crate::menus::invoke(app, ctx, id, params) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// The tab menu for the document a right click (or the menu key) named, at the place the request gave. It closes
/// when an entry runs, when it is dismissed and when its document is gone.
fn tab_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui, cx: &mut BodyCx<'_>, strip: &Response) {
    let Some((id, at)) = cx.keep::<TabMenu>().0 else { return };
    let Some(index) = document_index(app, id) else {
        cx.keep::<TabMenu>().0 = None;
        return;
    };
    let count = app.session.documents().len();
    let has_path = app.session.documents()[index].path.is_some();
    let items = tab_context_items(id, count, has_path);
    let mut entries = Vec::with_capacity(items.len() + TAB_MENU_SEPARATORS_AFTER.len());
    for (row, (label, _, _, enabled)) in items.iter().enumerate() {
        entries.push(Entry::new(row, tl!(*label)).enabled(*enabled));
        if TAB_MENU_SEPARATORS_AFTER.contains(&row) {
            entries.push(Entry::separator());
        }
    }
    let mut open = true;
    let picked =
        Menu::new(tl!("Document Tab")).opened_by(strip, Trigger::Manual).anchor(Anchor::Point(at)).open(Bind::bound(&mut open)).pick(ui, &entries).picked;
    if !open || picked.is_some() {
        cx.keep::<TabMenu>().0 = None;
    }
    if let Some((_, command, params, _)) = picked.and_then(|row| items.into_iter().nth(row)) {
        run(app, ui.ctx(), command, params);
    }
}

/// Rows of the document tab menu after which a separator is drawn: after Move to New Window and
/// after Open Document…, as in the reference (#11403).
pub(crate) const TAB_MENU_SEPARATORS_AFTER: [usize; 2] = [4, 6];

pub(crate) type TabMenuItem = (&'static str, &'static str, serde_json::Value, bool);

/// All tab actions go through the same guarded commands as the menu bar, including the
/// unsaved-changes prompt. The clicked tab's document `doc` is explicit even when another document is active.
/// Order and separators follow the reference application (#11403); see [`TAB_MENU_SEPARATORS_AFTER`].
pub(crate) fn tab_context_items(doc: DocId, count: usize, has_path: bool) -> [TabMenuItem; 8] {
    [
        ("Consolidate All to Here", "window.arrange.consolidateAllToTabs", json!({}), count > 1),
        ("Close", "file.close", json!({"document": doc.0}), true),
        ("Close All", "file.closeAll", json!({}), true),
        ("Close Others", "file.closeOthers", json!({"document": doc.0}), count > 1),
        ("Move to New Window", "window.arrange.floatInWindow", json!({"document": doc.0}), true),
        ("New Document…", "file.new", json!({}), true),
        ("Open Document…", "file.open", json!({}), true),
        (crate::menus::reveal_label(), "file.revealInFinder", json!({"document": doc.0}), has_path),
    ]
}

/// Photoshop's tab title as its two parts: the name, and "50% (Layer 1, RGB/8)" at `zoom` percent, with
/// "(Layer 1, Layer Mask/8)" when the mask is targeted and "(Quick Mask/8)" in Quick Mask mode; the Background
/// layer's name is omitted. The tab's modified mark shows unsaved changes, so the text carries none.
pub(crate) fn tab_title_at(app: &PhotocraftApp, i: usize, zoom: f32) -> (String, String) {
    use photocraft_engine::channel_cmds::ChannelTarget;
    let Some(st) = app.session.documents().get(i) else { return (String::new(), String::new()) };
    // Quick Mask mode targets the mask, not the layer (see `paint_target`): painting and Free
    // Transform change the red overlay, so the tab says so, as Photoshop's does.
    let quick_mask = st.doc.quick_mask.is_some() && st.channel_view.target == ChannelTarget::Composite;
    let active_layer = st.active_layer.and_then(|id| st.doc.layer(id)).filter(|l| !quick_mask && !st.doc.is_background(l.id));
    let mask = app.session.active_index() == Some(i) && app.ui.mask_target && active_layer.is_some_and(|l| l.mask.is_some());
    let model = if quick_mask {
        tl!("Quick Mask").to_string()
    } else if mask {
        tl!("Layer Mask").to_string()
    } else {
        mode_label(&st.doc).to_string()
    };
    let layer = active_layer.map(|l| format!("{}, ", l.name)).unwrap_or_default();
    // A save in progress: "100% (Layer 1, RGB/16) - Saving 45%", as Photoshop does (#3389).
    let saving = crate::jobs_ui::save_progress(app, st.doc.id).map(|s| format!(" - {s}")).unwrap_or_default();
<<<<<<< HEAD
    (st.doc.name.clone(), format!("{}% ({layer}{model}/{}){saving}", fmt_zoom(zoom), st.doc.depth.bits()))
}

/// The documents with a tab in the main window, in document order: every one not floating in its own window.
=======
    format!("{} @ {}% ({layer}{model}/{}){}{saving}", titled_name(st), fmt_zoom(zoom), st.doc.depth.bits(), if st.is_dirty() { "*" } else { "" })
}

/// The document's name as its tab and window title show it: "© name" when File Info marks it
/// Copyrighted, as Photoshop does (#13821).
fn titled_name(st: &photocraft_engine::DocState) -> String {
    let copyrighted = photocraft_engine::file_cmds::copyright_marked(st.doc.metadata.xmp.as_deref());
    format!("{}{}", if copyrighted { "\u{a9} " } else { "" }, st.doc.name)
}

/// The Studio strip's tab name: "name *" when unsaved, "name * - Saving 45%" while it saves (#3389).
fn studio_tab_name(app: &PhotocraftApp, st: &photocraft_engine::DocState) -> String {
    let saving = crate::jobs_ui::save_progress(app, st.doc.id).map(|s| format!(" - {s}")).unwrap_or_default();
    format!("{}{}{saving}", st.doc.name, if st.is_dirty() { " *" } else { "" })
}

/// Photoshop document tabs: "name @ 33.3% (RGB/8)" on a dark strip; active tab matches panels.
/// Tabs keep their whole titles, and the ones that don't fit move into a » menu at the end of the
/// strip; the active document's tab always stays on the strip (#1276, #4487).
fn pro_tabs(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> TabStrip {
    let t = crate::theme::Tokens::get(ui.ctx());
    let documents = docked_document_indices(app);
    let active_doc = app.session.active_index().filter(|_| app.jobs.focus.is_none());
    let active = active_doc.and_then(|doc| documents.iter().position(|&d| d == doc));
    let (mut activate, mut close) = (None, None);
    let mut tab_action = None;
    let mut tab_move = None;
    let mut tear_off = None;
    let tab_count = documents.len();
    let document_count = app.session.documents().len();
    let dragging = crate::layer_transfer::pointer_if_armed(app, ui.ctx());
    let mut drag_over = None;
    let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    let large_tabs = app.session.prefs().workspace.large_tabs;
    let font = egui::FontId::proportional(if large_tabs { 13.0 } else { 11.5 });
    let tab_h = if large_tabs { 34.0 } else { 26.0 };
    let (strip, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), tab_h), Sense::hover());
    ui.painter().rect_filled(strip, 0.0, t.tab_strip);
    // Files opening in the background (#210) are tabs too ("name (Opening… 45%)" with a progress
    // underline); they share the fit's index space, after the documents.
    let opening = crate::jobs_ui::open_tabs(app);
    let mut titles: Vec<String> = documents.iter().map(|&i| pro_tab_title(app, i)).collect();
    titles.extend(opening.iter().map(|(_, name, frac)| format!("{name} ({} {:.0}%)", tl!("Opening…"), frac * 100.0)));
    // Every tab's natural width, whether or not it stays on the strip.
    let natural: Vec<f32> = titles.iter().map(|title| ui.painter().layout_no_wrap(title.clone(), font.clone(), t.text).size().x + DOC_TAB_PAD).collect();
    let selected = app.jobs.focus.and_then(|job| opening.iter().position(|(open, _, _)| *open == job)).map_or_else(|| active.unwrap_or(0), |p| tab_count + p);
    let f = fit_doc_tabs(&natural, selected, strip.width());
    let mut x = strip.left();
    let mut placed: Vec<(usize, Rect)> = Vec::with_capacity(f.shown.len());
    for &(i, w) in &f.shown {
        let r = Rect::from_min_size(egui::pos2(x, strip.top()), egui::vec2(w, strip.height()));
        placed.push((i, r));
        x = r.right();
    }
    let mut doc_tabs = Vec::with_capacity(tab_count);
    let (mut focus_open, mut cancel_open) = (None, None);
    for &(i, r) in placed.iter().filter(|(i, _)| *i < tab_count) {
        let Some(&doc) = documents.get(i) else { continue };
        let Some(title) = titles.get(i) else { continue };
        let g = crate::tab_strip::elided(ui, title, font.clone(), t.text, (r.width() - DOC_TAB_PAD).max(1.0));
        let resp = ui.interact(r, ui.scope_id().with(("ptab", doc)), Sense::click_and_drag());
        let sel = Some(i) == active;
        if sel {
            ui.painter().rect_filled(r, 0.0, t.chrome);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.35));
        }
        if dragging.is_some_and(|p| r.contains(p)) {
            drag_over = Some(doc);
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }
        ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, t.separator));
        let (xr, at) = pro_tab_layout(r, g.size().y, mac);
        let xresp = ui.interact(xr, ui.scope_id().with(("ptabx", doc)), Sense::click());
        // Painted: name the tab and its × for accessibility (and so tests and agents can find them).
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, sel, g.text()));
        xresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Close")));
        crate::icons::paint(ui, xr, "x", 10.0, if xresp.hovered() { t.text } else { t.text_faint });
        ui.painter().galley_with_override_text_color(at, g, if sel { t.text } else { t.text_faint });
        let cut = natural.get(i).copied().unwrap_or(0.0) > r.width() + 0.5;
        let resp = if cut { resp.on_hover_text(title.as_str()) } else { resp };
        if tab_close_clicked(&resp, &xresp) {
            close = Some(doc);
        } else if resp.clicked() {
            activate = Some(doc);
        }
        match tab_drag(ui, &resp, i, r, strip, &placed, &t) {
            Some(TabDrop::Move(to)) => tab_move = Some((doc, move_target_after_filter(&documents, doc, to, document_count))),
            Some(TabDrop::Float(at)) => tear_off = Some((doc, at)),
            None => {}
        }
        let has_path = app.session.documents().get(doc).is_some_and(|d| d.path.is_some());
        resp.context_menu(|ui| {
            tab_action = tab_context_menu(ui, doc, document_count, has_path);
        });
        doc_tabs.push((doc, r));
    }
    for &(i, r) in placed.iter().filter(|(i, _)| *i >= tab_count) {
        let Some((job, _, frac)) = opening.get(i - tab_count) else { continue };
        let Some(title) = titles.get(i) else { continue };
        let g = crate::tab_strip::elided(ui, title, font.clone(), t.text, (r.width() - DOC_TAB_PAD).max(1.0));
        let resp = ui.interact(r, ui.scope_id().with(("ptabjob", job.0)), Sense::click());
        let sel = app.jobs.focus == Some(*job);
        if sel {
            ui.painter().rect_filled(r, 0.0, t.chrome);
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 0.0, t.hover.gamma_multiply(0.35));
        }
        ui.painter().line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, t.separator));
        crate::jobs_ui::tab_underline(ui, r, *frac, &t);
        let (xr, at) = pro_tab_layout(r, g.size().y, mac);
        let xresp = ui.interact(xr, ui.scope_id().with(("ptabjobx", job.0)), Sense::click());
        xresp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Close")));
        crate::icons::paint(ui, xr, "x", 10.0, if xresp.hovered() { t.text } else { t.text_faint });
        ui.painter().galley_with_override_text_color(at, g, if sel { t.text } else { t.text_faint });
        if xresp.on_hover_text(tl!("Cancel opening")).clicked() {
            cancel_open = Some(*job);
        } else if resp.clicked() {
            focus_open = Some(*job);
        }
    }
    if !f.overflow.is_empty() {
        // Pinned to the strip's right end, as Photoshop's is, not after the last tab (#8290).
        let left = (strip.right() - crate::tab_strip::CHEVRON_W).max(strip.left());
        let r = Rect::from_min_size(egui::pos2(left, strip.top()), egui::vec2(crate::tab_strip::CHEVRON_W, strip.height()));
        let labels: Vec<&str> = titles.iter().map(String::as_str).collect();
        let mut picked = None;
        // Like Photoshop's, the menu lists every open document, not only the hidden ones, and checks
        // the active one (#4487).
        let every: Vec<usize> = (0..labels.len()).collect();
        crate::tab_strip::overflow_button(ui, ui.scope_id().with("ptab-overflow"), r, tl!("More documents"), &labels, &every, Some(selected), &mut picked);
        match picked {
            Some(i) if i < tab_count => activate = documents.get(i).copied(),
            Some(i) => focus_open = opening.get(i - tab_count).map(|(job, _, _)| *job).or(focus_open),
            None => {}
        }
    }
    open_tab_clicks(app, activate, focus_open, cancel_open);
    if let Some(i) = drag_over {
        crate::layer_transfer::over_tab(app, ui.ctx(), i);
    }
    if let Some(i) = close {
        let _ = crate::menus::invoke(app, ui.ctx(), "file.close", json!({"document": i}));
    }
    if let Some((from, to)) = tab_move
        && let Err(e) = crate::menus::invoke(app, ui.ctx(), "document.move", json!({"document": from, "to": to}))
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    float_torn_off(app, ui.ctx(), tear_off);
    if let Some((id, params)) = tab_action
        && let Err(e) = crate::menus::invoke(app, ui.ctx(), id, params)
    {
        app.ui.status = e;
        app.ui.status_error = true;
    }
    TabStrip { rect: strip, tabs: doc_tabs }
}

/// Where the document tabs were drawn (opening files' tabs aren't slots): a file dropped on the
/// strip opens at the slot under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct TabStrip {
    pub rect: Rect,
    /// The document tabs shown, left to right, with their document index. When the tabs overflow
    /// into the » menu, some documents have no tab here.
    pub tabs: Vec<(usize, Rect)>,
}

impl TabStrip {
    /// The document position a drop at `x` opens at: before the first shown tab whose middle is
    /// right of it, else after the last shown tab.
    pub fn slot(&self, x: f32) -> usize {
        match self.tabs.iter().find(|(_, r)| r.center().x >= x) {
            Some(&(i, _)) => i,
            None => self.tabs.last().map_or(0, |&(i, _)| i.saturating_add(1)),
        }
    }
}

/// While files are dragged over the tab strip, an insertion line where they would open.
fn drop_slot_line(app: &mut PhotocraftApp, ui: &egui::Ui) {
    if ui.input(|i| i.raw.hovered_files.is_empty()) {
        return;
    }
    let at = app.cursor_pos(ui.ctx());
    let crate::file_open::DropTarget::Tabs(slot) = app.drop_target(ui.ctx(), at) else { return };
    let Some(tabs) = app.tab_strip.as_ref().map(|s| &s.tabs) else { return };
    let Some((r, after)) = tabs.iter().find(|(i, _)| *i == slot).map(|&(_, r)| (r, false)).or_else(|| tabs.last().map(|&(_, r)| (r, true))) else {
        return;
    };
    crate::widgets::drop_line(ui, r, after, true, &crate::theme::Tokens::get(ui.ctx()));
}

/// A Photoshop-style tab's close button and where its title starts. Photoshop puts the × after the
/// title on Windows (and so Linux) and before it on macOS (#619).
fn pro_tab_layout(tab: Rect, title_height: f32, mac: bool) -> (Rect, egui::Pos2) {
    let (x, title) = if mac { (tab.left() + 13.0, tab.left() + 26.0) } else { (tab.right() - 13.0, tab.left() + 16.0) };
    (Rect::from_center_size(egui::pos2(x, tab.center().y), egui::vec2(14.0, 14.0)), egui::pos2(title, tab.center().y - title_height / 2.0))
}

/// Repeating dot-grid texture for the canvas surround. A theme whose dots are its canvas colour
/// has no pattern to show, so it gets no texture at all rather than an invisible one (#4542).
pub(super) fn dots(ctx: &egui::Context, t: &crate::theme::Tokens) -> Option<egui::TextureId> {
    if t.bevel || t.canvas_dot == t.canvas {
        return None;
    }
    let key = egui::Id::unique(("canvas-dots", format!("{:?}", t.kind)));
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(key)) {
        return Some(tex.id());
    }
    let n = 22usize;
    let mut px = vec![Color32::TRANSPARENT; n * n];
    for (dx, dy, a) in [(0usize, 0usize, 255u8), (1, 0, 110), (0, 1, 110), (1, 1, 60)] {
        let c = t.canvas_dot;
        px[(n / 2 + dy) * n + n / 2 + dx] = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a);
    }
    let opts = TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::Repeat,
        mipmap_mode: None,
    };
    let tex = ctx.load_texture("canvas-dots", egui::ColorImage::new([n, n], px), opts);
    let id = tex.id();
    ctx.data_mut(|d| d.insert_temp(key, tex));
    Some(id)
}

pub(super) fn paint_dots(ui: &egui::Ui, rect: Rect, enabled: bool) {
    if !enabled {
        return;
    }
    let t = crate::theme::Tokens::get(ui.ctx());
    if let Some(id) = dots(ui.ctx(), &t) {
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(rect.width() / 22.0, rect.height() / 22.0));
        ui.painter_at(rect).image(id, rect, uv, Color32::WHITE);
    }
}

/// With no document open, double-clicking the empty workspace shows File › Open, as in Photoshop
/// (#5367), and so does the Home screen's blank background. Registered before anything drawn over
/// `area`, so the buttons and links there keep their own clicks; not a keyboard focus stop.
pub(super) fn open_on_double_click(app: &mut PhotocraftApp, ui: &egui::Ui, area: Rect) {
    if app.session.documents().is_empty() && ui.interact(area, ui.scope_id().with("empty-workspace"), Sense::CLICK).double_clicked() {
        let _ = app.open_dialog_file();
    }
}

/// Extra OS windows showing documents (multi-window / multi-monitor).
pub fn extra_windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let wins = app.ui.windows.clone();
    for w in wins.into_iter().filter(|w| w.open) {
        let Some(st) = app.session.documents().get(w.document) else { continue };
        let title = format!("{} — window {}", titled_name(st), w.id);
        let vid = egui::ViewportId::from_hash_of(("docwin", w.id));
        let mut builder = crate::os_window::builder().with_title(title).with_inner_size([800.0, 600.0]);
        if let Some([x, y]) = w.position {
            builder = builder.with_position([x, y]);
        }
        // A floating window owns its document's view, as the main canvas no longer shows it: it
        // draws `app.ui.views[doc]`, which the Navigator, the status bar's zoom and the View
        // commands edit, rather than a copy of its own that would ignore them (#11276). A New
        // Window for Document keeps its own view beside the main canvas's, read from
        // `app.ui.windows` rather than `wins`: a Scroll All Windows drag in a window drawn earlier
        // in this loop may have moved it since `wins` was taken.
        let owned = w.floating.then(|| app.ui.views.get(w.document).cloned()).flatten();
        let shares = owned.is_some();
        let mut view = owned.unwrap_or_else(|| app.ui.windows.iter().find(|x| x.id == w.id).map_or_else(|| w.view.clone(), |x| x.view.clone()));
        let mut close = false;
        let mut focused = false;
        ctx.show_viewport_immediate(vid, builder, |ui, class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                close = true;
            }
            focused = ui.ctx().input(|i| i.viewport().focused == Some(true));
            if focused && !w.focused && app.session.active_index() != Some(w.document) {
                app.session.set_active(w.document);
            }
            let (shown, rect) = document_window_canvas(app, ui, &w, view.clone(), focused);
            view = shown;
            // Preferences › Zoom Resizes Windows: a zoom wraps the window around the image.
            crate::zoom_window_fit::follow(app, ui.ctx(), class, vid, &w, &mut view, rect);
        });
        if shares && let Some(v) = app.ui.views.get_mut(w.document) {
            *v = view.clone();
        }
        if let Some(win) = app.ui.windows.iter_mut().find(|x| x.id == w.id) {
            win.view = view;
            win.focused = focused;
            if close {
                win.open = false;
            }
        }
    }
    app.ui.windows.retain(|w| w.open);
}

/// The canvas of document window `w` (Window › New Window), drawn in its own viewport. Returns the
/// view after this frame's input and the canvas rect.
///
/// Dialogs are modal, but their `egui::Modal` only blocks the viewport it is drawn in, the main
/// window. Without a modal layer of its own, this window's canvas would take presses and drags
/// under a dialog: a Move drag here committed a History step the dialog's Cancel never undid.
/// So while a dialog is open the window gets an empty, transparent modal of its own, and its
/// canvas is as inert as the main one's; the dialog's own gestures on the image (pan, zoom,
/// eyedroppers) read the raw pointer and still work here, as they do there.
pub(super) fn document_window_canvas(
    app: &mut PhotocraftApp,
    ui: &mut egui::Ui,
    w: &crate::state::DocWindow,
    view: crate::state::View,
    focused: bool,
) -> (crate::state::View, Rect) {
    if !app.ui.dialogs.is_empty() {
        egui::Modal::new(egui::Id::unique(("docwin-dialog-block", w.id))).backdrop_color(Color32::TRANSPARENT).frame(egui::Frame::NONE).show(ui.ctx(), |_| {});
    }
    let mut out = (view.clone(), Rect::NOTHING);
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::theme::Tokens::get(ui.ctx()).canvas)).show(ui, |ui| {
        let rect = ui.available_rect_before_wrap();
        app.drawing_window = Some(w.id);
        out = (canvas_view(app, ui, w.document, rect, view, focused), rect);
        app.drawing_window = None;
    });
    out
}

/// Window › Arrange tiles: each document shown with its tile in `rect`, or `None` when the
/// active document fills the area.
>>>>>>> upstream/main
pub(super) fn docked_document_indices(app: &PhotocraftApp) -> Vec<usize> {
    app.session.documents().iter().enumerate().filter(|(_, st)| !crate::view_cmds::document_is_floating(app, st.doc.id)).map(|(i, _)| i).collect()
}

/// The document the main window's single canvas shows: the active one, or the first docked one while the active
/// document floats.
pub(super) fn main_canvas_document(app: &PhotocraftApp) -> Option<usize> {
    let active = app.session.active()?;
    if !crate::view_cmds::document_is_floating(app, active.doc.id) {
        return app.session.active_index();
    }
    docked_document_indices(app).into_iter().next()
}

/// Convert a final tab index in the visible (docked-only) strip to the document list index,
/// keeping hidden floating documents in their existing relative order.
pub(super) fn move_target_after_filter(visible: &[usize], from: usize, to: usize, document_count: usize) -> usize {
    let remaining: Vec<usize> = visible.iter().copied().filter(|&doc| doc != from).collect();
    match remaining.get(to) {
        Some(&doc) if doc > from => doc - 1,
        Some(&doc) => doc,
        None => document_count.saturating_sub(1),
    }
}

#[cfg(test)]
#[path = "../file_info_copyright_title_tests.rs"]
mod file_info_copyright_title_tests;
