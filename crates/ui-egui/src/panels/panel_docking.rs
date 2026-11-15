//! PhotoCraft's workspace on the framework dock: the `DockArea` pass over the panel registry, the one path that
//! changes the workspace ([`apply`]), the panel queries the menus and tests read, the control protocol's
//! `window.panel.*` ids, and the saved layout's load, snapshot and one-time migration of pre-workspace saves.
//!
//! `app.ui.docking` is the live [`Workspace`]; the registry (`panels::registry`) names every panel it may hold.
//! The lock is `app.workspaces.set.locked`.

use craft::behavior::dock::{Action, DockEdge, DockPlacement, Layout, Node, PanelPlace, PanelState, Permissions, SplitSize, Target};
use craft::behavior::{DockSide, EditEvent, EditEvents, IconId};
use craft::ui::contract::{Bind, BodyCx, SizeLimits};
use craft::ui::docking::{
    DockCommands, DockConfig, DockEvent, DockPanels, DockScene, DockSizes, DockTo, DragUnit, FloatSpace, GroupAt, PanelSpec, actions, move_actions,
    request_actions,
};
use craft::ui::menu::PanelRow;
use craft::ui::patterns::dock::{DockArea, DockAreaOutput};
use craft::ui::primitives::IconSpec;
use serde_json::{Map, Value, json};

use super::registry::{self, CANVAS, CANVAS_MIN, LEGACY_GROUPS};
use crate::PhotocraftApp;

/// The saved arrangement: layout, hidden panels with their return places, and the floated panels' docked places.
pub(crate) type Workspace = craft::behavior::dock::Workspace<String>;

/// Where `window.panel.float` puts a panel when the request gives no place (`docs/control-protocol.md:773`).
const FLOAT_AT: [f32; 2] = [400.0, 140.0];

/// The size `window.panel.float` gives a panel that has no float size of its own (`docs/control-protocol.md:774`).
const FLOAT_SIZE: [f32; 2] = [300.0, 400.0];

/// What the workspace allows `panel`, as the dock and every command apply it.
fn permissions(panel: &String) -> Permissions {
    registry::permissions(panel)
}

/// Whether `panel` is in the workspace, docked or floating.
pub(crate) fn is_shown(app: &PhotocraftApp, id: &str) -> bool {
    app.ui.docking.layout.contains(&id.to_owned())
}

/// Whether `panel` shows: the active tab of its group, and neither the group nor its window minimized.
pub(crate) fn is_front(app: &PhotocraftApp, id: &str) -> bool {
    app.ui.docking.layout.panel_state(&id.to_owned()) == PanelState::Front
}

/// Shows or hides panel `id` (an id or a label). `Some(true)` opens a hidden panel at its place, and brings a shown
/// one to the front, expanding its minimized group or window; `Some(false)` closes it; `None` shows a panel that is
/// hidden, minimized or behind another tab and closes one that is in front. Returns whether the panel shows.
pub(crate) fn show_panel(app: &mut PhotocraftApp, id: &str, show: Option<bool>) -> Result<bool, String> {
    let panel = registry::normalize(id).ok_or_else(|| format!("`{id}` is not a registered panel"))?;
    let show = show.unwrap_or_else(|| !is_front(app, panel));
    if !show && !is_shown(app, panel) {
        return Ok(false);
    }
    let action = if show { Action::open(panel.to_owned(), None) } else { Action::close(panel.to_owned()) };
    apply(app, vec![action])?;
    Ok(show)
}

/// Applies `actions` to the workspace in order, all or none, under the lock: every change of the workspace from the
/// menus, the rail, the control protocol, the dock pass and the migration goes through here. A closed Timeline stops
/// playing, and the layout is remembered. Returns the panels the actions closed.
///
/// While the dock pass runs on the app's context, the workspace is on screen: actions that pass the checks go to the
/// pass, which hands them back to [`show`] to apply here when it ends, in the same frame.
pub(crate) fn apply(app: &mut PhotocraftApp, actions: Vec<Action<String>>) -> Result<Vec<String>, String> {
    apply_during(app, actions, false)
}

/// [`apply`], for the actions of a dock pass whose divider drag or window move is still `dragging`: the steps of a
/// drag change the workspace, and the layout is remembered once, when the drag commits ([`show`]).
fn apply_during(app: &mut PhotocraftApp, actions: Vec<Action<String>>, dragging: bool) -> Result<Vec<String>, String> {
    let (next, closed) = planned(app, &actions)?;
    if request_actions(app.ctx(), actions).is_ok() {
        return Ok(closed);
    }
    app.ui.docking = next;
    stop_closed_timeline(app, &closed);
    if !dragging {
        crate::dock::persist(app);
    }
    Ok(closed)
}

/// Whether a workspace gesture of the pass (a divider drag, a window move) is still running after it: its last
/// event is its begin or a step, not its commit or cancel. The area's own one-step edits belong to no gesture.
fn dragging(edit: &EditEvents<Workspace>) -> bool {
    edit.iter().filter(|event| event.id().is_some()).last().is_some_and(|event| matches!(event, EditEvent::Begin { .. } | EditEvent::Update { .. }))
}

/// Whether [`apply`] would take `actions`: the same refusals, on a copy of the workspace. Changes nothing.
pub(crate) fn check(app: &PhotocraftApp, actions: &[Action<String>]) -> Result<(), String> {
    planned(app, actions).map(|_| ())
}

/// The workspace `actions` make of the live one, and the panels they close; the refusals of [`apply`] and [`check`].
fn planned(app: &PhotocraftApp, actions: &[Action<String>]) -> Result<(Workspace, Vec<String>), String> {
    let mut next = app.ui.docking.clone();
    let closed = next.apply_all(actions.iter().cloned(), app.workspaces.set.locked, permissions).map_err(|error| error.to_string())?;
    // The model leaves the panel registry to the application: an action may not bring in an unknown panel.
    if let Some(unknown) = next.layout.panels().into_iter().find(|panel| !registry::registered(panel)) {
        return Err(format!("`{unknown}` is not a registered panel"));
    }
    Ok((next, closed))
}

/// The Timeline plays only while it is in the workspace.
fn stop_closed_timeline(app: &mut PhotocraftApp, closed: &[String]) {
    if closed.iter().any(|panel| panel == "timeline") {
        super::timeline_ui::stop(app);
    }
}

/// What the dock reads of `panel`: its name, icon, least size, group heights and float size.
fn spec(panel: &str) -> PanelSpec {
    if panel == CANVAS {
        let least = SizeLimits::at_least(egui::vec2(CANVAS_MIN[0], CANVAS_MIN[1]));
        return PanelSpec::new(tl!("Documents").to_string()).canvas().limits(least).allow_native(false);
    }
    let Some(entry) = registry::entry(panel) else { return PanelSpec::new(panel.to_owned()) };
    let mut spec = PanelSpec::new((entry.label)())
        .icon(IconSpec::svg(IconId::new(entry.icon)))
        .limits(SizeLimits::at_least(egui::vec2(entry.min[0], entry.min[1])));
    if let Some(heights) = entry.heights {
        spec = spec.heights(heights);
    }
    if let Some([width, height]) = entry.float_size {
        spec = spec.float_size(egui::vec2(width, height));
    }
    spec
}

/// The registry's panels as the dock asks for them.
struct Panels<'a> {
    app: &'a mut PhotocraftApp,
}

impl DockPanels<String> for Panels<'_> {
    fn spec(&self, panel: &String) -> PanelSpec {
        spec(panel)
    }

    fn permissions(&self, panel: &String) -> Permissions {
        permissions(panel)
    }

    fn body(&mut self, panel: &String, ui: &mut egui::Ui, _: &mut BodyCx<'_>) {
        body(self.app, ui, panel);
    }

    /// The canvas group is the document area: it draws its own tabs, rail and documents in the group's rect.
    fn group_view(&mut self, panels: &[String], _: usize, ui: &mut egui::Ui, at: GroupAt, _: &mut BodyCx<'_>) -> Option<f32> {
        if !panels.iter().any(|panel| panel == CANVAS) {
            return None;
        }
        match at {
            // Protected, the canvas never sits in a flyout, which is what measures.
            GroupAt::Measure(_) => Some(CANVAS_MIN[1]),
            GroupAt::Show(rect) => {
                #[cfg(test)]
                probe::record(ui.ctx(), CANVAS, rect, false);
                crate::canvas::document_area(self.app, ui);
                Some(rect.height())
            }
        }
    }

    fn group_header(&mut self, panels: &[String], _: usize, ui: &mut egui::Ui, _: &mut BodyCx<'_>) {
        #[cfg(test)]
        if let Some(first) = panels.first() {
            probe::record(ui.ctx(), first, ui.max_rect(), true);
        }
        let _ = (panels, ui);
    }

    fn panel_rows(&mut self, panel: &String) -> Vec<PanelRow> {
        registry::entry(panel).map(|entry| (entry.rows)(self.app)).unwrap_or_default()
    }
}

/// Draws panel `id`'s body in `ui`: in a vertical scroll area, unless the panel lays out its own scrolling list or
/// sizes itself to the group.
pub(crate) fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: &str) {
    let Some(entry) = registry::entry(id) else { return };
    #[cfg(test)]
    probe::record(ui.ctx(), id, ui.max_rect(), false);
    if entry.scrolls_itself {
        ui.set_min_height(ui.available_height().max(0.0));
        (entry.body)(app, ui);
    } else {
        // No scroll fade: it darkens real colours, swatches and previews at the edge (#2726).
        ui.spacing_mut().scroll.fade.strength = 0.0;
        egui::ScrollArea::vertical().id_salt(("panel body", id)).auto_shrink([false, false]).show(ui, |ui| (entry.body)(app, ui));
    }
}

/// Draws the workspace, the canvas included, in the room `ui` has left.
///
/// The area shows the live workspace and writes none of it. Every change of the pass reaches [`apply`] once, after
/// the pass: the area's own (tab picks, drops, menu rows, divider drags, window moves), then what the bodies asked
/// for while it ran (`show_panel` and the other [`apply`] callers). Each pass applies only its own input: the area
/// reads the pass's pointer and the files dropped on it, and a second pass in the same frame gets neither again, so
/// nothing applies twice. A divider drag or window move changes the workspace at each step and remembers the layout
/// once, when it commits: the area's `edit` says whether its gesture is still running.
pub(crate) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    #[cfg(test)]
    probe::clear(ui.ctx());
    let ctx = ui.ctx().clone();
    let workspace = app.ui.docking.clone();
    let locked = app.workspaces.set.locked;
    let auto_collapse = app.session.prefs().workspace.auto_collapse_icon_panels;
    let config = DockConfig::default().group_order_items(true).native_windows(cfg!(not(target_arch = "wasm32")));
    let output: DockAreaOutput<String> = {
        let mut panels = Panels { app: &mut *app };
        let mut area = DockArea::new(tl!("Panels").to_string(), Bind::given(workspace), &mut panels, config);
        if locked {
            area = area.locked();
        }
        if auto_collapse {
            area = area.auto_collapse();
        }
        area.show(ui)
    };
    #[cfg(test)]
    probe::record_windows(&ctx, &app.ui.docking.layout, &output.windows);
    let DockAreaOutput { actions, requested, events, error, edit, window_files, .. } = output;
    if let Some(error) = error {
        app.ui.status = format!("{}: {}", tl!("Invalid panel layout"), error.error);
    }
    let dragging = dragging(&edit);
    let changes: Vec<Action<String>> = actions.into_iter().chain(requested).collect();
    let applied = !changes.is_empty()
        && match apply_during(app, changes, dragging) {
            Ok(_) => true,
            Err(error) => {
                app.ui.status = error;
                false
            }
        };
    // A drag that ends with no change of its own in this pass still saves the steps it held.
    if !applied && !dragging && edit.ended().is_some() {
        crate::dock::persist(app);
    }
    for event in events {
        if matches!(event, DockEvent::ToggleAutoCollapse) {
            app.session.prefs.edit(|prefs| prefs.workspace.auto_collapse_icon_panels = !prefs.workspace.auto_collapse_icon_panels);
        }
    }
    // Files dropped on a floating panel's own window open as documents, and the main window comes forward.
    if !window_files.is_empty() {
        app.open_dropped(&ctx, window_files, crate::file_open::DropTo::Window);
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }
}

/// The actions that give the end dock the total width `total`, its columns scaled within their bounds. Empty when
/// there is no end dock, the width is not a positive number or the columns already have it.
pub(crate) fn width_actions(app: &PhotocraftApp, ctx: &egui::Context, total: f32) -> Vec<Action<String>> {
    if !total.is_finite() || total <= 0.0 {
        return Vec::new();
    }
    DockSizes::of(ctx).resize_side(&app.ui.docking.layout, DockEdge::End, &|panel: &String| spec(panel), total)
}

// ---------------------------------------------------------------------------------------------------------------
// The control protocol's panel ids (`docs/control-protocol.md:770-800`).
// ---------------------------------------------------------------------------------------------------------------

/// Runs a `window.panel.*`, `window.toggle.<panel>`, `type.panels.*` or `ui.dock` command; `None` for an id that is
/// not one of them. `window.panel.modifierKeys` is the Window menu's (its params latch the modifier keys), and the
/// `window.workspace.*` ids run where the menu runs them.
pub(crate) fn command(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let result = match id {
        "window.panel.modifierKeys" => return None,
        "ui.dock" => ui_dock(app, params),
        "window.panel" | "window.panel.activate" => fields(params).and_then(|fields| {
            let panel = required_panel(&fields)?;
            toggled(app, &panel, Some(true))
        }),
        "window.panel.group" => group_command(app, params),
        "window.panel.layout" => match params.get("action") {
            Some(action) => serde_json::from_value::<Action<String>>(action.clone()).map_err(|error| error.to_string()).and_then(|action| run(app, vec![action])),
            None => Err("action is required".into()),
        },
        "window.panel.move" => move_command(app, params),
        "window.panel.float" => float_command(app, params),
        "window.panel.dock" => dock_command(app, params),
        "window.panel.close" => one_panel(app, params, Action::close),
        "window.panel.closeGroup" => one_panel(app, params, Action::close_group),
        "window.panel.closeOthers" => one_panel(app, params, Action::close_others),
        "window.panel.minimize" => minimize_command(app, params),
        "window.panel.window" => window_command(app, params),
        // Opening the Character or Paragraph panel from the type tool never hides it (`view_cmds/dispatch.rs:303`).
        "type.panels.character" => toggled(app, "character", Some(true)),
        "type.panels.paragraph" => toggled(app, "paragraph", Some(true)),
        _ => {
            let name = id
                .strip_prefix("window.panel.")
                .or_else(|| id.strip_prefix("window.toggle."))
                .or_else(|| id.strip_prefix("type.panels.").filter(|name| matches!(*name, "characterStyles" | "paragraphStyles" | "glyphs")))?;
            let panel = registry::normalize(name)?;
            show_param(params).and_then(|show| toggled(app, panel, show))
        }
    };
    Some(result)
}

/// The `show` (or `on`) param of a panel toggle: `None` toggles.
fn show_param(params: &Value) -> Result<Option<bool>, String> {
    match params.get("show").or_else(|| params.get("on")) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value.as_bool().map(Some).ok_or_else(|| "show must be a boolean".into()),
    }
}

/// [`show_panel`] with the protocol's answer.
fn toggled(app: &mut PhotocraftApp, panel: &str, show: Option<bool>) -> Result<Value, String> {
    let shown = show_panel(app, panel, show)?;
    Ok(json!({"panel": panel, "shown": shown, "layout": app.ui.docking.layout}))
}

/// Runs `actions` through [`apply`] with the protocol's answer.
fn run(app: &mut PhotocraftApp, actions: Vec<Action<String>>) -> Result<Value, String> {
    let closed = apply(app, actions)?;
    Ok(json!({"layout": app.ui.docking.layout, "closed": closed}))
}

/// The params as an object, panels named by id or label and `onto` standing for `anchor`. The canvas may be an
/// anchor or a target, since docks and frames open beside it.
fn fields(params: &Value) -> Result<Map<String, Value>, String> {
    let mut fields = match params {
        Value::Null => Map::new(),
        Value::Object(fields) => fields.clone(),
        _ => return Err("params must be an object".into()),
    };
    if let Some(onto) = fields.remove("onto") {
        fields.entry("anchor").or_insert(onto);
    }
    for key in ["panel", "anchor", "before", "target"] {
        match fields.get(key) {
            None | Some(Value::Null) => {}
            Some(value) => {
                // Docks and frames open beside the canvas, so it may be an anchor or a target.
                let canvas = key != "panel" && value.as_str().is_some_and(|raw| raw.trim().eq_ignore_ascii_case(CANVAS));
                let panel = if canvas { Some(CANVAS) } else { value.as_str().and_then(registry::normalize) };
                let panel = panel.ok_or_else(|| format!("{key} must name a registered panel"))?;
                fields.insert(key.into(), json!(panel));
            }
        }
    }
    Ok(fields)
}

/// The panel a normalized param names.
fn named(fields: &Map<String, Value>, key: &str) -> Option<String> {
    fields.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn required_panel(fields: &Map<String, Value>) -> Result<String, String> {
    named(fields, "panel").ok_or_else(|| "panel must name a registered panel".into())
}

fn flag(fields: &Map<String, Value>, key: &str) -> bool {
    fields.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// What a move or float takes along: the panel, its group (`group: true`) or its dock column (`column: true`).
fn unit(fields: &Map<String, Value>, panel: String) -> DragUnit<String> {
    if flag(fields, "column") {
        DragUnit::Column(panel)
    } else if flag(fields, "group") {
        DragUnit::Group(panel)
    } else {
        DragUnit::Panel(panel)
    }
}

/// Where a `zone` param puts a unit beside `anchor`: `tab` (the default) joins its tabs before `before`; `center`
/// joins them at the end; `left`, `right`, `top`/`above` and `bottom`/`below` go beside its group.
fn destination(fields: &Map<String, Value>, anchor: String) -> Result<DockTo<String>, String> {
    let zone = fields.get("zone").and_then(Value::as_str).unwrap_or("tab");
    let rtl = crate::direction::is_rtl();
    let side = match zone {
        "tab" => return Ok(DockTo::tabs(anchor, named(fields, "before"))),
        "center" => DockSide::Center,
        "left" if rtl => DockSide::End,
        "left" => DockSide::Start,
        "right" if rtl => DockSide::Start,
        "right" => DockSide::End,
        "top" | "above" => DockSide::Top,
        "bottom" | "below" => DockSide::Bottom,
        _ => return Err("zone must be tab, center, left, right, top, bottom, above or below".into()),
    };
    Ok(DockTo::beside(anchor, side))
}

/// Runs `f` with the dock scene of the live workspace, for the framework's panel commands and moves.
fn with_scene<R>(app: &PhotocraftApp, f: impl FnOnce(&DockScene<'_, String>) -> R) -> R {
    // The moves and Dock Panel built here never read the space's bounds: `window.panel.float` gives its own rect.
    let sizes = DockSizes::of(app.ctx());
    let space = FloatSpace::new(egui::Rect::NOTHING, &sizes);
    let policy = |panel: &String| permissions(panel);
    let workspace = &app.ui.docking;
    let scene = DockScene::new(&workspace.layout, &space, app.workspaces.set.locked, &policy).floated(&workspace.floated);
    f(&scene)
}

/// `window.panel.move {panel, anchor, zone?, before?, group?, placement?}`.
fn move_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let fields = fields(params)?;
    let panel = required_panel(&fields)?;
    let anchor = named(&fields, "anchor").ok_or("anchor must name a registered panel")?;
    let actions = match fields.get("placement") {
        Some(placement) => {
            let placement: DockPlacement<String> = serde_json::from_value(placement.clone()).map_err(|error| error.to_string())?;
            vec![if flag(&fields, "group") {
                Action::move_group(panel, Target::at(anchor, placement))
            } else {
                Action::move_beside(panel, anchor, placement)
            }]
        }
        None => {
            let to = destination(&fields, anchor)?;
            let unit = unit(&fields, panel);
            with_scene(app, |scene| move_actions(scene, &unit, to)).map_err(|error| error.to_string())?
        }
    };
    run(app, actions)
}

/// The window rect of a float: the `x`, `y`, `width` and `height` params, or (400, 140) and the panel's float size,
/// 300 × 400 without one.
fn float_rect(fields: &Map<String, Value>, panel: &str) -> Result<egui::Rect, String> {
    let size = registry::entry(panel).and_then(|entry| entry.float_size).unwrap_or(FLOAT_SIZE);
    let number = |key: &str, default: f32| match fields.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(value) => value.as_f64().map(|v| v as f32).ok_or_else(|| format!("{key} must be a number")),
    };
    let (x, y) = (number("x", FLOAT_AT[0])?, number("y", FLOAT_AT[1])?);
    let (width, height) = (number("width", size[0])?, number("height", size[1])?);
    if ![x, y, width, height].iter().all(|v| v.is_finite()) || width <= 0.0 || height <= 0.0 {
        return Err("float geometry must be finite, with a positive width and height".into());
    }
    Ok(egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(width, height)))
}

/// `window.panel.float {panel, x?, y?, width?, height?, group?, column?}`.
fn float_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let fields = fields(params)?;
    let panel = required_panel(&fields)?;
    let to = Target::float(float_rect(&fields, &panel)?);
    let action = if flag(&fields, "column") {
        Action::move_column(panel, to)
    } else if flag(&fields, "group") {
        Action::move_group(panel, to)
    } else {
        Action::move_panel(panel, to)
    };
    run(app, vec![action])
}

/// `window.panel.dock {panel, anchor?, zone?, before?, group?, side?}`: without an anchor, a floating panel goes back
/// where it floated from when that place is still docked (Dock Panel), or into the dock on `side`; with one, beside
/// or into the anchor's group.
fn dock_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let fields = fields(params)?;
    let panel = required_panel(&fields)?;
    let actions = match named(&fields, "anchor") {
        Some(anchor) => {
            let to = destination(&fields, anchor)?;
            let unit = unit(&fields, panel);
            with_scene(app, |scene| move_actions(scene, &unit, to))
        }
        None => {
            let side = match fields.get("side").and_then(Value::as_str) {
                None => None,
                Some("start") => Some(DockEdge::Start),
                Some("end") => Some(DockEdge::End),
                Some(_) => return Err("side must be start or end".into()),
            };
            match side {
                Some(edge) => {
                    let unit = unit(&fields, panel);
                    with_scene(app, |scene| move_actions(scene, &unit, DockTo::Dock(edge)))
                }
                None if flag(&fields, "group") => with_scene(app, |scene| move_actions(scene, &DragUnit::Group(panel), DockTo::Docked)),
                None => with_scene(app, |scene| actions(scene, DockCommands::DockPanel, &panel)),
            }
        }
    }
    .map_err(|error| error.to_string())?;
    run(app, actions)
}

/// A command on one panel: `{panel}`.
fn one_panel(app: &mut PhotocraftApp, params: &Value, action: fn(String) -> Action<String>) -> Result<Value, String> {
    let panel = required_panel(&fields(params)?)?;
    run(app, vec![action(panel)])
}

/// Whether the group or floating window holding `panel` is minimized.
fn minimized(layout: &Layout<String>, panel: &String) -> bool {
    match layout.place(panel) {
        Some(PanelPlace::Floating { window, .. }) => layout.floating.get(window).is_some_and(|floating| floating.minimized),
        Some(place) => layout.group(&place).is_some_and(|group| group.minimized),
        None => false,
    }
}

/// `window.panel.minimize {panel, minimized?}`: without `minimized`, toggles.
fn minimize_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let fields = fields(params)?;
    let panel = required_panel(&fields)?;
    let on = match fields.get("minimized") {
        None | Some(Value::Null) => !minimized(&app.ui.docking.layout, &panel),
        Some(value) => value.as_bool().ok_or("minimized must be a boolean")?,
    };
    run(app, vec![Action::set_minimized(panel, on)])
}

/// `window.panel.window {panel, native?}`: floats a docked panel's group, whose window the dock gives an OS window of
/// its own on the desktop; `native: false` keeps a floating window in the main window.
fn window_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    if cfg!(target_arch = "wasm32") {
        return Err("floating panels stay in the page on the web".into());
    }
    let fields = fields(params)?;
    let panel = required_panel(&fields)?;
    let native = fields.get("native").and_then(Value::as_bool).unwrap_or(true);
    let floating = matches!(app.ui.docking.layout.place(&panel), Some(PanelPlace::Floating { .. }));
    let actions = match (native, floating) {
        (true, true) => Vec::new(),
        (true, false) => vec![Action::move_group(panel.clone(), Target::float(float_rect(&fields, &panel)?))],
        (false, _) => vec![Action::set_native(panel, None)],
    };
    run(app, actions)
}

/// `ui.dock {action}` or `ui.dock {operation, …}` with the operations `open`, `close`, `activate`, `float`,
/// `moveFloating`, `move`, `setStackOpen`, `resizeStack` and `resizeSplit`.
fn ui_dock(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let fields = fields(params)?;
    if let Some(action) = fields.get("action") {
        let action = serde_json::from_value::<Action<String>>(action.clone()).map_err(|error| error.to_string())?;
        return run(app, vec![action]);
    }
    let operation = fields.get("operation").and_then(Value::as_str).ok_or("operation is required")?;
    let panel = || required_panel(&fields);
    let rect = || -> Result<egui::Rect, String> {
        match fields.get("rect").and_then(Value::as_array) {
            Some(rect) => {
                let at = |i: usize| rect.get(i).and_then(Value::as_f64).map(|v| v as f32).ok_or("rect must be [x, y, width, height]");
                let rect = egui::Rect::from_min_size(egui::pos2(at(0)?, at(1)?), egui::vec2(at(2)?, at(3)?));
                if !rect.is_finite() || !rect.is_positive() {
                    return Err("float geometry must be finite, with a positive width and height".into());
                }
                Ok(rect)
            }
            None => float_rect(&fields, &panel()?),
        }
    };
    let action = match operation {
        "open" => return toggled(app, &panel()?, Some(true)),
        "close" => Action::close(panel()?),
        "activate" => Action::activate(panel()?),
        "float" => Action::move_panel(panel()?, Target::float(rect()?)),
        "moveFloating" => Action::move_floating(panel()?, rect()?),
        "move" => {
            let anchor = named(&fields, "anchor").ok_or("anchor must name a registered panel")?;
            let placement = fields.get("placement").cloned().unwrap_or_else(|| json!({"Tab": {"before": null}}));
            let placement: DockPlacement<String> = serde_json::from_value(placement).map_err(|error| error.to_string())?;
            Action::move_beside(panel()?, anchor, placement)
        }
        "setStackOpen" => Action::set_stack_open(panel()?, fields.get("open").and_then(Value::as_bool).ok_or("open must be a boolean")?),
        "resizeStack" => Action::resize_stack(panel()?, fields.get("height").and_then(Value::as_f64).map(|height| height as f32)),
        "resizeSplit" => {
            let path: Vec<bool> = serde_json::from_value(fields.get("path").cloned().unwrap_or(json!([]))).map_err(|error| error.to_string())?;
            let size: SplitSize = serde_json::from_value(fields.get("size").cloned().ok_or("size is required")?).map_err(|error| error.to_string())?;
            Action::resize_split(path, size)
        }
        _ => return Err("operation must be open, close, activate, float, moveFloating, move, setStackOpen, resizeStack or resizeSplit".into()),
    };
    run(app, vec![action])
}

/// The panels of the group, tab node or accordion entry holding `panel`, in tab order; empty when it is hidden.
fn group_members(layout: &Layout<String>, panel: &String) -> Vec<String> {
    let Some(place) = layout.place(panel) else { return Vec::new() };
    if let Some(group) = layout.group(&place) {
        return group.panels.clone();
    }
    match layout.node_of(&place) {
        Some(Node::Tabs { panels, .. }) => panels.clone(),
        _ => vec![panel.clone()],
    }
}

/// `window.panel.group {panel, operation}`: commands for a panel's whole group.
fn group_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let panel = params.get("panel").and_then(Value::as_str).and_then(registry::normalize).ok_or("panel must name a registered panel")?.to_owned();
    let operation = params.get("operation").and_then(Value::as_str).ok_or("operation is required")?;
    if app.workspaces.set.locked {
        return Err("workspace is locked".into());
    }
    let layout = &app.ui.docking.layout;
    let members = group_members(layout, &panel);
    if members.is_empty() {
        return Err("panel is hidden".into());
    }
    let actions = match operation {
        "close" => vec![Action::close_group(panel.clone())],
        "collapse" => vec![Action::set_minimized(panel.clone(), true)],
        "expand" => vec![Action::set_minimized(panel.clone(), false)],
        "move" => {
            let fields = fields(params)?;
            let anchor = named(&fields, "anchor").ok_or("anchor must name a registered panel")?;
            if members.contains(&anchor) {
                return Err("group cannot be moved into itself".into());
            }
            let to = destination(&fields, anchor)?;
            let unit = DragUnit::Group(panel.clone());
            let mut moves = with_scene(app, |scene| move_actions(scene, &unit, to)).map_err(|error| error.to_string())?;
            moves.push(Action::activate(panel.clone()));
            moves
        }
        "up" | "down" => {
            let up = operation == "up";
            if !matches!(layout.place(&panel), Some(PanelPlace::Dock { .. })) {
                return Err("floating groups cannot move within the dock".into());
            }
            if !layout.can_step_group(&panel, up, permissions) {
                return Err("group is already at the edge".into());
            }
            vec![Action::step_group(panel.clone(), up), Action::activate(panel.clone())]
        }
        _ => return Err("operation must be close, collapse, expand, move, up or down".into()),
    };
    apply(app, actions)?;
    Ok(json!({"layout": app.ui.docking.layout}))
}

// ---------------------------------------------------------------------------------------------------------------
// The saved layout.
// ---------------------------------------------------------------------------------------------------------------

/// A saved `docking` value from a newer version than this one reads.
fn newer(docking: &Value) -> bool {
    docking.get("version").and_then(Value::as_u64).is_some_and(|version| version > u64::from(Workspace::VERSION))
}

/// Loads the workspace of a saved layout (`prefs.panelLayout` or a saved workspace) into `app.ui.docking`.
///
/// A readable `docking` workspace is fitted to the registry. One from a newer version, or one that does not read,
/// shows the default; [`snapshot`] writes a newer one back unchanged until the arrangement changes. A save from
/// before the workspace (`panels`, `dockTabs`, `dock`, `timelineOpen`) migrates once onto the default.
pub(crate) fn load(app: &mut PhotocraftApp, saved: &Value) {
    app.ui.docking.replace(loaded(saved));
}

/// [`load`] at launch: Brush Settings is a transient window, so a remembered workspace that holds it loads with it
/// closed (its place kept for when it opens). A workspace switch loads it as saved.
pub(crate) fn load_at_launch(app: &mut PhotocraftApp, saved: &Value) {
    let mut workspace = loaded(saved);
    let transient = "brushSettings".to_owned();
    if workspace.layout.contains(&transient)
        && let Err(error) = workspace.apply(Action::close(transient), false, permissions)
    {
        app.ui.status = error.to_string();
    }
    app.ui.docking.replace(workspace);
}

/// The workspace of a saved layout, fitted to the registry.
fn loaded(saved: &Value) -> Workspace {
    let default = registry::default_workspace();
    let mut workspace = match saved.get("docking") {
        Some(docking) if docking.is_object() => match serde_json::from_value::<Workspace>(docking.clone()) {
            Ok(workspace) if workspace.validate().is_ok() => workspace,
            _ => default.clone(),
        },
        _ => migrated(saved, &default),
    };
    workspace.reconcile(|panel: &String| registry::registered(panel), permissions, &default);
    workspace
}

/// A save from before the workspace on the default workspace: its group flags, selected tabs, collapsed groups and
/// closed tabs, and an open Timeline. The saved group order and heights do not carry over; anything that does not
/// apply leaves the default.
fn migrated(saved: &Value, default: &Workspace) -> Workspace {
    let mut dock = saved.get("dock").cloned();
    // Every group keeps the one tab order and its built-in height.
    if let Some(dock) = dock.as_mut().and_then(Value::as_object_mut) {
        dock.remove("order");
        dock.remove("heights");
    }
    let Ok(mut actions) = legacy_on(&default.layout, saved.get("panels"), saved.get("dockTabs"), dock.as_ref()) else { return default.clone() };
    if saved.get("timelineOpen").and_then(Value::as_bool) == Some(true) {
        actions.push(Action::open("timeline".to_owned(), None));
    }
    let mut workspace = default.clone();
    match workspace.apply_all(actions, false, permissions) {
        Ok(_) => workspace,
        Err(_) => default.clone(),
    }
}

/// What a saved layout holds: the chrome flags and the workspace. A newer saved workspace that shows as the default
/// is written back as it was saved, so this version never overwrites it until the user changes the arrangement.
pub(crate) fn snapshot(app: &PhotocraftApp) -> Value {
    let kept = app.session.prefs().panel_layout.get("docking").filter(|docking| newer(docking)).cloned();
    let docking = match kept {
        Some(kept) if app.ui.docking == registry::default_workspace() => kept,
        _ => json!(app.ui.docking),
    };
    json!({"panels": app.ui.panels, "docking": docking})
}

// ---------------------------------------------------------------------------------------------------------------
// The old panel and dock words of saves and of `ui.set` (`panels.<group>`, `dockTabs.<group>`, `dock`).
// ---------------------------------------------------------------------------------------------------------------

/// The first panel of legacy group `key` that the layout holds.
fn shown_member(layout: &Layout<String>, panels: &[&str]) -> Option<String> {
    panels.iter().map(|panel| (*panel).to_owned()).find(|panel| layout.contains(panel))
}

/// Whether `panel` is the selected tab of the group or tab node holding it (an accordion entry always is).
fn selected(layout: &Layout<String>, panel: &String) -> bool {
    let Some(place) = layout.place(panel) else { return false };
    if let Some(group) = layout.group(&place) {
        return group.active_panel() == Some(panel);
    }
    match layout.node_of(&place) {
        Some(Node::Tabs { panels, active, .. }) => panels.get(*active) == Some(panel),
        _ => true,
    }
}

/// The workspace actions of the old panel and dock words, each a JSON object or absent, on the live workspace:
/// `panels.<group>` shows a hidden group's panels or closes a shown group's; `dockTabs.<group>: i` selects the i-th
/// panel of the group's tab order; `dock.hidden_tabs` closes the named tabs; `dock.collapsed` (every collapsed group)
/// minimizes those groups and expands the others. `dock.order` and `dock.heights` are refused: groups are arranged
/// with `window.panel.layout`.
pub(crate) fn legacy_actions(
    app: &PhotocraftApp,
    panels: Option<&Value>,
    dock_tabs: Option<&Value>,
    dock: Option<&Value>,
) -> Result<Vec<Action<String>>, String> {
    if dock.is_some_and(|dock| dock.get("order").is_some() || dock.get("heights").is_some()) {
        return Err("dock order/heights cannot replace a customized layout; use window.panel.layout".into());
    }
    legacy_on(&app.ui.docking.layout, panels, dock_tabs, dock)
}

/// [`legacy_actions`] on `layout`, without the order and heights refusal.
fn legacy_on(layout: &Layout<String>, panels: Option<&Value>, dock_tabs: Option<&Value>, dock: Option<&Value>) -> Result<Vec<Action<String>>, String> {
    let mut actions = Vec::new();
    // Each group's panels as they will be after the request's opens and closes, for the collapse step.
    let mut shown: Vec<(&str, Option<String>)> = LEGACY_GROUPS.iter().map(|(key, members)| (*key, shown_member(layout, members))).collect();
    let flag = |key: &str| panels.and_then(|panels| panels.get(key)).and_then(Value::as_bool);
    for ((key, members), (_, first)) in LEGACY_GROUPS.iter().zip(shown.iter_mut()) {
        match flag(key) {
            // A shown group stays as it is; a hidden one shows all of its panels at their places.
            Some(true) if first.is_none() => {
                actions.extend(members.iter().map(|panel| Action::open((*panel).to_owned(), None)));
                *first = members.first().map(|panel| (*panel).to_owned());
            }
            Some(false) => {
                actions.extend(members.iter().map(|panel| (*panel).to_owned()).filter(|panel| layout.contains(panel)).map(Action::close));
                *first = None;
            }
            _ => {}
        }
    }
    let hidden_tabs = dock.and_then(|dock| dock.get("hidden_tabs").or_else(|| dock.get("hiddenTabs"))).and_then(Value::as_object);
    for (key, names) in hidden_tabs.into_iter().flatten() {
        let Some((_, members)) = LEGACY_GROUPS.iter().find(|(group, _)| *group == key.as_str()) else { continue };
        for name in names.as_array().into_iter().flatten().filter_map(Value::as_str) {
            if let Some(panel) = registry::normalize(name).filter(|panel| members.contains(panel)).map(str::to_owned)
                && layout.contains(&panel)
                && !actions.contains(&Action::close(panel.clone()))
            {
                actions.push(Action::close(panel));
            }
        }
    }
    for (key, index) in dock_tabs.and_then(Value::as_object).into_iter().flatten() {
        let Some((_, members)) = LEGACY_GROUPS.iter().find(|(group, _)| *group == key.as_str()) else { continue };
        let index = index.as_u64().ok_or("dockTabs entries must be tab indices")?;
        let panel = usize::try_from(index).ok().and_then(|index| members.get(index)).ok_or("dockTabs index is outside the panel group")?;
        if flag(key) == Some(false) {
            return Err("cannot select a tab in a hidden group".into());
        }
        let panel = (*panel).to_owned();
        actions.push(if layout.contains(&panel) { Action::activate(panel) } else { Action::open(panel, None) });
    }
    if let Some(collapsed) = dock.and_then(|dock| dock.get("collapsed")).and_then(Value::as_array) {
        let collapsed: Vec<&str> = collapsed.iter().filter_map(Value::as_str).collect();
        for (key, first) in &shown {
            let Some(first) = first else { continue };
            let minimize = collapsed.contains(key);
            if minimize != minimized(layout, first) || !layout.contains(first) {
                actions.push(Action::set_minimized(first.clone(), minimize));
            }
        }
    }
    Ok(actions)
}

/// The old panel and dock words of the live workspace, for readers of `ui.inspect`: `panels.<group>` is whether any
/// of the group's panels shows, `dockTabs.<group>` the index of its selected panel in the group's tab order, for the
/// groups that show.
pub(crate) fn legacy_view(app: &PhotocraftApp) -> Value {
    let layout = &app.ui.docking.layout;
    let mut panels = Map::new();
    let mut tabs = Map::new();
    for (key, members) in LEGACY_GROUPS {
        let laid_out: Vec<usize> = (0..members.len()).filter(|i| layout.contains(&members[*i].to_owned())).collect();
        panels.insert((*key).into(), json!(!laid_out.is_empty()));
        if let Some(index) = laid_out.iter().copied().find(|i| selected(layout, &members[*i].to_owned())).or_else(|| laid_out.first().copied()) {
            tabs.insert((*key).into(), json!(index));
        }
    }
    json!({"panels": panels, "dockTabs": tabs})
}

/// Where tests find panels as the last dock pass drew them. Only tests record anything.
#[cfg(test)]
pub(crate) mod probe {
    use super::*;
    use egui::Rect;

    pub(crate) use super::body;

    /// The id of the pass's drawn rects: each a panel's body, or (`header`) the header of the group it leads.
    fn drawn_id() -> egui::Id {
        egui::Id::new("photocraft dock drawn")
    }

    type Drawn = Vec<(String, Rect, bool)>;

    fn drawn(ctx: &egui::Context) -> Drawn {
        ctx.data(|data| data.get_temp::<Drawn>(drawn_id())).unwrap_or_default()
    }

    /// Starts the pass's record.
    pub(super) fn clear(ctx: &egui::Context) {
        ctx.data_mut(|data| data.insert_temp(drawn_id(), Drawn::new()));
    }

    /// Notes the body of `panel`, or the header of the group it leads, at `rect`.
    pub(super) fn record(ctx: &egui::Context, panel: &str, rect: Rect, header: bool) {
        ctx.data_mut(|data| data.get_temp_mut_or_default::<Drawn>(drawn_id()).push((panel.to_owned(), rect, header)));
    }

    /// The body of `panel`, when the last pass drew it.
    pub(crate) fn panel_rect(ctx: &egui::Context, panel: &str) -> Option<Rect> {
        drawn(ctx).into_iter().find(|(id, _, header)| id == panel && !header).map(|(_, rect, _)| rect)
    }

    /// The panels of the tab group holding `panel`, in tab order.
    pub(crate) fn group_of(app: &PhotocraftApp, panel: &str) -> Vec<String> {
        group_members(&app.ui.docking.layout, &panel.to_owned())
    }

    /// The tab group holding `panel` as the last pass drew it: its header and its shown panel's body.
    pub(crate) fn group_rect(app: &PhotocraftApp, ctx: &egui::Context, panel: &str) -> Option<Rect> {
        let members = group_of(app, panel);
        drawn(ctx).into_iter().filter(|(id, _, _)| members.contains(id)).map(|(_, rect, _)| rect).reduce(|a, b| a.union(b))
    }

    /// [`group_rect`] of the first laid-out panel of the old group whose first panel is `first`.
    pub(crate) fn native_group_rect(app: &PhotocraftApp, ctx: &egui::Context, first: &str) -> Option<Rect> {
        let (_, members) = LEGACY_GROUPS.iter().find(|(_, members)| members.first() == Some(&first))?;
        members.iter().find(|panel| is_shown(app, panel)).and_then(|panel| group_rect(app, ctx, panel))
    }

    /// The id of the OS windows the last pass showed floating windows in, by each panel they hold.
    fn windows_id() -> egui::Id {
        egui::Id::new("photocraft dock windows")
    }

    type Windows = Vec<(String, egui::ViewportId)>;

    /// Notes the OS windows the dock reported, `windows` by a panel of each, for every panel of their floating
    /// windows in `layout`, the layout the pass showed.
    pub(super) fn record_windows(ctx: &egui::Context, layout: &Layout<String>, windows: &[(String, egui::ViewportId)]) {
        let by_panel: Windows = windows
            .iter()
            .flat_map(|(anchor, viewport)| {
                let window = layout.floating.iter().find(|window| window.contains(anchor));
                window.into_iter().flat_map(|window| window.panels().cloned()).map(move |panel| (panel, *viewport))
            })
            .collect();
        ctx.data_mut(|data| data.insert_temp(windows_id(), by_panel));
    }

    /// The OS window the floating window holding `panel` showed in on the last pass, as the dock reported it.
    pub(crate) fn native_viewport(ctx: &egui::Context, panel: &str) -> Option<egui::ViewportId> {
        let windows = ctx.data(|data| data.get_temp::<Windows>(windows_id())).unwrap_or_default();
        windows.into_iter().find(|(shown, _)| shown == panel).map(|(_, viewport)| viewport)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod native_tests;
