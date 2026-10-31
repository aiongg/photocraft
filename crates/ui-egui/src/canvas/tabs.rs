use super::start_screen::start_screen;
use super::textures::retain_gpu_documents;
use super::{Color32, PhotocraftApp, Pos2, Rect, Sense, TextureOptions, canvas_view, fmt_zoom, json, mode_label, pos2};
use craft_ui::theme::StrokeExt;

/// Tabs + canvas for the active document, or the start screen; then drops layers dragged onto
/// another document (`layer_transfer`).
pub fn document_area(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    crate::distort_ui::drop_foreign(app);
    documents(app, ui);
    crate::layer_transfer::finish(app, ui);
    // Once a frame, not per document view: the right-click or options-bar Brush Preset picker.
    crate::paint_mouse::show_picker(app, ui.ctx());
}

/// Window › Arrange tiles: each document shown with its tile in `rect`, or `None` when the
/// active document fills the area.
pub(crate) fn arranged_cells(app: &PhotocraftApp, rect: Rect) -> Option<Vec<(usize, Rect)>> {
    let idx = app.session.active_index()?;
    let n = app.session.documents().len();
    let cells = crate::view_cmds::cells(&app.ui.view.arrange, rect, n)?;
    // Each document keeps its tile: a click into another one activates it where it is (#6017; it
    // used to move to the first tile). With more documents than tiles, the shown run stays put
    // while the active document is in it, and otherwise slides only as far as it must.
    let start = app.arrange_start.min(n.saturating_sub(cells.len()));
    let start = if idx < start { idx } else { start.max((idx + 1).saturating_sub(cells.len())) };
    Some((start..n).zip(cells).collect())
}

fn documents(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    app.drop_canvas_rect = None;
    let previous_strip = app.tab_strip.take();
    retain_gpu_documents(app);
    crate::transform_tool::track_steps(app, ui.ctx());
    crate::cas_transform::tick(app, ui.ctx());
    crate::symmetry_ui::track(app, ui.ctx());
    let n = app.session.documents().len();
    // Files opening in the background (#210) have tabs before they have documents.
    let opening = !app.jobs.opens.is_empty();
    if !opening && app.ui.chrome.shows_home(n, app.session.prefs().general.auto_show_home_screen) {
        start_screen(app, ui);
        return;
    }
    if n == 0 && !opening {
        // Auto show the Home Screen is off: an empty workspace, like Photoshop.
        let area = ui.available_rect_before_wrap();
        paint_dots(ui, area, app.session.prefs().interface.show_canvas_dots);
        open_on_double_click(app, ui, area);
        return;
    }
    if !app.ui.view.hides_tabs() || opening {
        // The tabs sit directly against the canvas. The parent's default vertical item spacing
        // would otherwise leave a visible gap below the tab strip (#2188).
        let spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 0.0;
        app.tab_strip = Some(tabs(app, ui, previous_strip));
        ui.spacing_mut().item_spacing.y = spacing;
    }
    if let Some(job) = app.jobs.focus.or_else(|| (n == 0).then(|| app.jobs.opens.last().map(|o| o.job)).flatten()) {
        crate::jobs_ui::open_card(app, ui, job);
        return;
    }
    let Some(idx) = app.session.active_index() else { return };
    let rect = ui.available_rect_before_wrap();
    app.last_canvas_rect = rect;
    app.drop_canvas_rect = Some(rect);
    // Window › Arrange: tiled / n-up layouts show several documents side by side; the active one
    // takes input, a click elsewhere activates that document.
    if let Some(shown) = arranged_cells(app, rect) {
        app.arrange_start = shown.first().map_or(0, |(d, _)| *d);
        let t = crate::theme::Tokens::get(ui.ctx());
        for (d, cell) in shown {
            let cell = cell.shrink(1.0);
            let view = app.ui.views[d].clone();
            let v = canvas_view(app, ui, d, cell, view, d == idx);
            if d != idx {
                app.ui.views[d] = v;
                if ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| cell.contains(p))) {
                    app.session.set_active(d);
                }
            }
            let stroke = if d == idx {
                craft_ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.accent)
            } else {
                craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), Color32::from_black_alpha(160))
            };
            ui.painter().rect_stroke(cell, craft_ui::theme::scale::Radius::R0.px(), stroke, egui::StrokeKind::Inside);
        }
        return;
    }
    let view = app.ui.views[idx].clone();
    canvas_view(app, ui, idx, rect, view, true);
}

/// What a tab of the document strip stands for: an open document, or a file still opening in the
/// background (#210). Documents are keyed by identity, not position, so a reorder reads the same
/// after the strip's order changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DocTab {
    Doc(photocraft_doc::DocId),
    Opening(photocraft_engine::jobs::JobId),
}

/// Document index of the tab `key`, if it is a document.
fn doc_index(app: &PhotocraftApp, key: DocTab) -> Option<usize> {
    let DocTab::Doc(id) = key else { return None };
    app.session.documents().iter().position(|st| st.doc.id == id)
}

/// The document strip. Its tabs keep their whole titles, and the ones that don't fit move into a
/// » menu at the end of the strip; the active document's tab always stays on the strip (#1276,
/// #4487). Files opening in the background are tabs after the documents, with a progress bar and
/// a × that cancels the open. Returns where the document tabs are.
pub(super) fn tabs(app: &mut PhotocraftApp, ui: &mut egui::Ui, previous: Option<TabStrip>) -> TabStrip {
    use craft_ui::edit::EditEvent;
    use craft_ui::menu::{ContextMenu, MenuEntry, MenuItem, OpenAt};
    use craft_ui::tab_strip::{Tab, TabEvent, TabKind, TabStrip as LibStrip, TabStripText};
    let tab_count = app.session.documents().len();
    let opening = crate::jobs_ui::open_tabs(app);
    let mut strip_tabs: Vec<Tab<DocTab>> = (0..tab_count)
        .map(|i| {
            let st = &app.session.documents()[i];
            Tab::new(DocTab::Doc(st.doc.id), tab_label(app, i)).modified(st.is_dirty()).closable(true)
        })
        .collect();
    strip_tabs.extend(opening.iter().map(|(job, name, frac)| {
        Tab::new(DocTab::Opening(*job), format!("{name} ({} {:.0}%)", tl!("Opening…"), frac * 100.0))
            .closable(true)
            .close_label(tl!("Cancel opening"))
            .progress(Some(*frac))
    }));
    let selected = match app.jobs.focus.filter(|job| opening.iter().any(|(open, _, _)| open == job)) {
        Some(job) => Some(DocTab::Opening(job)),
        None => app.session.active_index().and_then(|i| app.session.documents().get(i)).map(|st| DocTab::Doc(st.doc.id)),
    };
    // Layers dragged over a tab show its document (`layer_transfer`) and outline it; files dragged
    // over the strip show a line where they would open. Both read where last frame's tabs were.
    let dragging = crate::layer_transfer::pointer_if_armed(app, ui.ctx());
    let highlight = dragging
        .and_then(|p| previous.as_ref().and_then(|s| s.tabs.iter().find(|(_, r)| r.contains(p))))
        .and_then(|&(i, _)| app.session.documents().get(i))
        .map(|st| DocTab::Doc(st.doc.id));
    let marker = drop_slot(app, ui, previous);
    let text = TabStripText {
        more: tl!("More documents").to_owned(),
        close: tl!("Close").to_owned(),
        modified: tl!("{tab}, modified").to_owned(),
        ..TabStripText::default()
    };
    let mut builder = LibStrip::new("documents", tl!("Documents"), TabKind::Document)
        .large(app.session.prefs().workspace.large_tabs)
        .tear_off(true)
        .insertion_marker(marker)
        .drop_highlight(highlight)
        .text(text);
    if let Some(key) = &selected {
        builder = builder.selected(key);
    }
    let mut out = builder.show(ui, &strip_tabs);
    let menu_id = out.response.id.with("tab-menu");
    let (mut activate, mut focus_open, mut cancel_open) = (None, None, None);
    let (mut close, mut tab_move, mut tear_off) = (None, None, None);
    let mut open_menu = None;
    for event in std::mem::take(&mut out.events) {
        match event {
            TabEvent::Activate(DocTab::Opening(job)) => focus_open = Some(job),
            TabEvent::Activate(key) => activate = doc_index(app, key),
            TabEvent::Close(DocTab::Opening(job)) => cancel_open = Some(job),
            TabEvent::Close(key) => close = doc_index(app, key),
            TabEvent::ContextMenu(request) => {
                if let Some(DocTab::Doc(id)) = request.tab {
                    ui.data_mut(|d| d.insert_temp(menu_id, id));
                    open_menu = Some(request.pointer.map_or(OpenAt::Rect(request.anchor), OpenAt::Pointer));
                }
            }
            TabEvent::Reorder(EditEvent::Commit { value: mv, .. } | EditEvent::Set { value: mv, .. }) => {
                if let Some(from) = mv.items.first().and_then(|&key| doc_index(app, key)) {
                    // A tab dropped next to an opening one lands after the last document.
                    let at = doc_index(app, mv.target).map_or(tab_count, |t| if mv.position == craft_ui::behavior::DropPosition::After { t + 1 } else { t });
                    let to = tab_move_target(from, at);
                    if to != from {
                        tab_move = Some((from, to));
                    }
                }
            }
            TabEvent::DroppedOut { tab, pointer } => {
                if let Some(doc) = doc_index(app, tab) {
                    // The pointer is in this window's points; the OS wants screen coordinates. Keep
                    // the new window's title bar under the pointer, as the ghost showed.
                    let at = ui.ctx().input(|i| i.viewport().inner_rect).map(|inner| {
                        let s = inner.min + pointer.to_vec2() - egui::vec2(40.0, 12.0);
                        [s.x, s.y]
                    });
                    tear_off = Some((doc, at));
                }
            }
            _ => {}
        }
    }
    // A tab dragged off the strip shows where its window will open.
    if let Some(drag) = &out.dragging_out {
        let t = crate::theme::Tokens::get(ui.ctx());
        let ghost = Rect::from_min_size(drag.pointer - egui::vec2(40.0, 12.0), egui::vec2(240.0, 160.0));
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, ui.scope_id().with("tab-tear-off")));
        painter.rect_filled(ghost, t.radius_sm, t.accent.gamma_multiply(0.12));
        painter.rect_stroke(ghost, t.radius_sm, craft_ui::theme::scale::Stroke::W1_5.egui(ui.ctx(), t.accent), egui::StrokeKind::Inside);
    }
    // Every tab action goes through the guarded File commands, as the menu bar does.
    let target = ui.data(|d| d.get_temp::<photocraft_doc::DocId>(menu_id)).and_then(|id| doc_index(app, DocTab::Doc(id)));
    let items = target.map(|i| tab_context_items(i, tab_count, app.session.documents()[i].path.is_some()));
    let entries: Vec<MenuEntry<usize>> =
        items.iter().flatten().enumerate().map(|(n, (label, _, _, enabled))| MenuItem::new(tl!(*label).to_string(), n).enabled(*enabled).into()).collect();
    let chosen = ContextMenu::new(&entries).own_gestures(false).open_at(open_menu).show(ui, &out.response).chosen;
    let tab_action = chosen.and_then(|n| items.as_ref().and_then(|items| items.get(n))).map(|(_, id, params, _)| (*id, params.clone()));
    let drag_over = dragging.and_then(|p| out.tab_at(p)).and_then(|key| doc_index(app, key));
    let doc_tabs: Vec<(usize, Rect)> = out.tabs.iter().filter_map(|&(key, r)| doc_index(app, key).map(|i| (i, r))).collect();
    let rect = out.response.rect;
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
    TabStrip { rect, tabs: doc_tabs }
}

/// The slot files being dragged over the strip would open at, for the strip's drop line.
fn drop_slot(app: &mut PhotocraftApp, ui: &egui::Ui, previous: Option<TabStrip>) -> Option<usize> {
    if ui.input(|i| i.raw.hovered_files.is_empty()) {
        return None;
    }
    // `drop_target` reads where the strip was last frame.
    app.tab_strip = previous;
    let at = app.services.cursor_pos.as_mut().and_then(|f| f(ui.ctx()));
    let target = app.drop_target(ui.ctx(), at);
    app.tab_strip = None;
    match target {
        crate::file_open::DropTarget::Tabs(slot) => Some(slot),
        _ => None,
    }
}

/// All tab actions go through the same guarded File commands as the menu bar, including the
/// unsaved-changes prompt. The clicked tab is explicit even when another document is active.
pub(super) fn tab_context_items(index: usize, count: usize, has_path: bool) -> [(&'static str, &'static str, serde_json::Value, bool); 4] {
    [
        ("Close", "file.close", json!({"document": index}), true),
        ("Close Others", "file.closeOthers", json!({"document": index}), count > 1),
        ("Close All", "file.closeAll", json!({}), true),
        (crate::menus::reveal_label(), "file.revealInFinder", json!({"document": index}), has_path),
    ]
}

/// Float a torn-off tab's document in its own window (`window.arrange.floatInWindow`).
fn float_torn_off(app: &mut PhotocraftApp, ctx: &egui::Context, tear_off: Option<(usize, Option<[f32; 2]>)>) {
    let Some((doc, at)) = tear_off else { return };
    let params = match at {
        Some(at) => json!({"document": doc, "position": at}),
        None => json!({"document": doc}),
    };
    if let Err(e) = crate::menus::invoke(app, ctx, "window.arrange.floatInWindow", params) {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

/// The `document.move` target for dropping the tab at `from` at insertion index `k`: the slot
/// shifts down when the dragged tab itself is removed first; next to itself it stays put.
pub(super) fn tab_move_target(from: usize, k: usize) -> usize {
    if k > from { k - 1 } else { k }
}

/// Apply tab-strip clicks: a document tab shows that document, an opening tab its progress, and
/// an opening tab's × cancels the open (closing the half-open tab).
fn open_tab_clicks(
    app: &mut PhotocraftApp,
    activate: Option<usize>,
    focus_open: Option<photocraft_engine::jobs::JobId>,
    cancel_open: Option<photocraft_engine::jobs::JobId>,
) {
    if let Some(i) = activate {
        app.session.set_active(i);
        app.jobs.focus = None;
    }
    if let Some(job) = focus_open {
        app.jobs.focus = Some(job);
    }
    if let Some(job) = cancel_open {
        crate::jobs_ui::cancel(app, job);
    }
}

/// Photoshop's tab title: "name @ 50% (Layer 1, RGB/8)", "(Layer 1, Layer Mask/8)" when the mask
/// is targeted, "(Quick Mask/8)" in Quick Mask mode; the Background layer's name is omitted.
#[cfg(test)]
pub(super) fn pro_tab_title(app: &PhotocraftApp, i: usize) -> String {
    tab_title_at(app, i, app.ui.views.get(i).map_or(100.0, |v| v.zoom * 100.0))
}

/// The tab title at `zoom` percent (a workspace with its own view, such as Select and Mask).
pub(crate) fn tab_title_at(app: &PhotocraftApp, i: usize, zoom: f32) -> String {
    let Some(st) = app.session.documents().get(i) else { return String::new() };
    let (title, saving) = title_parts(app, i, zoom);
    format!("{title}{}{saving}", if st.is_dirty() { "*" } else { "" })
}

/// The strip's label: the title and a running save, without the modified mark the tab draws itself.
pub(super) fn tab_label(app: &PhotocraftApp, i: usize) -> String {
    let (title, saving) = title_parts(app, i, app.ui.views.get(i).map_or(100.0, |v| v.zoom * 100.0));
    format!("{title}{saving}")
}

/// "name @ 50% (Layer 1, RGB/8)" and " - Saving 45%" (empty unless a save runs).
fn title_parts(app: &PhotocraftApp, i: usize, zoom: f32) -> (String, String) {
    use photocraft_engine::channel_cmds::ChannelTarget;
    let Some(st) = app.session.documents().get(i) else { return Default::default() };
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
    // A save in progress: "name @ 100% (Layer 1, RGB/16) - Saving 45%", as Photoshop does (#3389).
    let saving = crate::jobs_ui::save_progress(app, st.doc.id).map(|s| format!(" - {s}")).unwrap_or_default();
    (format!("{} @ {}% ({layer}{model}/{})", st.doc.name, fmt_zoom(zoom), st.doc.depth.bits()), saving)
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
        let title = format!("{} — window {}", st.doc.name, w.id);
        let vid = egui::ViewportId::from_hash_of(("docwin", w.id));
        let mut builder = egui::ViewportBuilder::default().with_title(title).with_inner_size([800.0, 600.0]);
        if let Some([x, y]) = w.position {
            builder = builder.with_position([x, y]);
        }
        let mut view = w.view.clone();
        let mut close = false;
        ctx.show_viewport_immediate(vid, builder, |ui, class| {
            if ui.ctx().input(|i| i.viewport().close_requested()) {
                close = true;
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(crate::theme::Tokens::get(ui.ctx()).canvas)).show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                view = canvas_view(app, ui, w.document, rect, view.clone(), false);
                // Preferences › Zoom Resizes Windows: a zoom wraps the window around the image.
                crate::zoom_window_fit::follow(app, ui.ctx(), class, vid, &w, &mut view, rect);
            });
        });
        if let Some(win) = app.ui.windows.iter_mut().find(|x| x.id == w.id) {
            win.view = view;
            if close {
                win.open = false;
            }
        }
    }
    app.ui.windows.retain(|w| w.open);
}
