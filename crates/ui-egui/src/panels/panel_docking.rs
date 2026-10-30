//! Application-owned panel IDs and commands around the shared docking workspace.
//!
//! craft-ui draws the window's panels and the canvas as one layout: a horizontal split of the
//! protected `canvas` panel and a dock at the logical end edge. Until the first docking
//! operation the layout is built each frame from the native dock's state (`crate::dock`), so its
//! groups, tab order, selection, heights and collapse follow that state; from then on
//! `app.ui.docking` holds a craft-ui [`Workspace`]. The layout, hidden panels and their return
//! places, and the lock are craft-ui's; PhotoCraft keeps the panel registry, the collapsed custom
//! groups and where a floated panel docks back.

use crate::dock::Group;
use craft_ui::docking::{Action, Column, DockCommand, DockHeights, FloatSpace, Floating, Layout, Location, Node, PanelState, Permissions, Placement, Side};
use craft_ui::layout::{SplitAxis, SplitSize};
use craft_ui::theme::StrokeExt;
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;

/// The saved arrangement: layout, hidden panels with their return places, and the lock.
pub(crate) type Workspace = craft_ui::docking::Workspace<String>;

type Returns = std::collections::BTreeMap<String, Location<String>>;

const PANELS: &[(&str, &str, Group)] = &[
    ("color", "Color", Group::Color),
    ("swatches", "Swatches", Group::Color),
    ("gradients", "Gradients", Group::Color),
    ("patterns", "Patterns", Group::Color),
    ("properties", "Properties", Group::Properties),
    ("adjustments", "Adjustments", Group::Properties),
    ("character", "Character", Group::Character),
    ("paragraph", "Paragraph", Group::Character),
    ("navigator", "Navigator", Group::Navigator),
    ("histogram", "Histogram", Group::Navigator),
    ("info", "Info", Group::Navigator),
    ("history", "History", Group::History),
    ("actions", "Actions", Group::History),
    ("layerComps", "Layer Comps", Group::History),
    ("layers", "Layers", Group::Layers),
    ("channels", "Channels", Group::Layers),
    ("paths", "Paths", Group::Layers),
    ("brushes", "Brushes", Group::Brushes),
];

/// PhotoCraft's wording of craft-ui's docking labels, translated through `tl!`. The others keep
/// craft-ui's English.
const DOCK_LABELS: &[(&str, &str)] = &[
    ("dock.float", "Float Panel"),
    ("dock.close", "Close Panel"),
    ("dock.movePanelTo", "Group with"),
    ("dock.panels", "Panels"),
    ("dock.resize", "Resize panels"),
    ("dock.resizeWindow", "Resize panel window"),
    ("dock.closeGroup", "Close Tab Group"),
    ("dock.floatGroup", "Float Tab Group"),
    ("dock.minimize", "Collapse Panel Group"),
    ("dock.expand", "Expand Panel Group"),
    ("dock.moveGroupUp", "Move Group Up"),
    ("dock.moveGroupDown", "Move Group Down"),
    ("dock.panelMenu", "Panel menu"),
];

/// Where a panel floats when a command gives no rect: craft-ui places it 24 points inside this
/// area, at (400, 140) on the window.
const FLOAT_AREA: egui::Rect = egui::Rect::from_min_max(egui::pos2(376.0, 116.0), egui::pos2(1400.0, 900.0));
const FLOAT_SIZE: egui::Vec2 = egui::vec2(300.0, 400.0);

fn area_id() -> egui::Id {
    egui::Id::unique("photocraft-panel-docking")
}

#[cfg(test)]
fn bodies_id() -> egui::Id {
    area_id().with("bodies")
}

/// The Layers panel's least width as of the last frame (points).
#[cfg(test)]
pub(crate) fn last_layers_min(ctx: &egui::Context) -> f32 {
    ctx.data(|data| data.get_temp::<f32>(bodies_id().with("layers-min"))).unwrap_or_default()
}

/// The body of each panel drawn last frame, the canvas included (screen points).
#[cfg(test)]
pub(crate) fn last_bodies(ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
    ctx.data(|data| data.get_temp::<Vec<(String, egui::Rect)>>(bodies_id())).unwrap_or_default()
}

/// The panel that holds the document area. It never moves, floats, closes or takes tabs, and
/// docks and frames open beside it.
pub(crate) const CANVAS: &str = "canvas";

/// Every other panel may move, float and close; the workspace lock limits them all.
fn permissions(id: &String) -> Permissions {
    if id == CANVAS { Permissions { accepts_splits: true, ..Permissions::PROTECTED } } else { Permissions::default() }
}

/// The width of the dock column in a layout built from the native dock.
fn default_width(pro: bool) -> f32 {
    if pro { 290.0 } else { 300.0 }
}

/// The window id of the Studio themes' floating Properties group in a layout built each frame:
/// the same every time, with the top bit set, where craft-ui's fresh ids never reach.
const PROPERTIES_WINDOW: u64 = (1 << 63) | 0x5052_4f50;

/// The canvas beside `dock`, which is `width` wide, at the logical end edge.
fn with_canvas(dock: Node<String>, width: f32) -> Node<String> {
    Node::Split {
        axis: SplitAxis::Horizontal,
        size: SplitSize::FixedSecond(width),
        first: Box::new(Node::Tabs { panels: vec![CANVAS.to_owned()], active: 0 }),
        second: Box::new(dock),
    }
}

/// A layout saved before the canvas joined it, beside the canvas as it was beside the document
/// area. Layouts that hold the canvas are returned unchanged.
fn beside_canvas(mut layout: Layout<String>) -> Layout<String> {
    if !layout.panels().iter().any(|p| p.as_str() == CANVAS) {
        layout.root = Some(match layout.root.take() {
            Some(root) => with_canvas(root, default_width(false)),
            None => Node::Tabs { panels: vec![CANVAS.to_owned()], active: 0 },
        });
    }
    layout
}

fn docked(layout: &Layout<String>) -> Layout<String> {
    Layout { root: layout.root.clone(), floating: Vec::new() }
}

pub(crate) fn normalize(raw: &str) -> Option<&'static str> {
    PANELS.iter().find(|(id, label, _)| id.eq_ignore_ascii_case(raw.trim()) || label.eq_ignore_ascii_case(raw.trim())).map(|p| p.0)
}

fn registered(id: &str) -> bool {
    id == CANVAS || normalize(id) == Some(id)
}

pub(crate) fn valid(layout: &Layout<String>) -> bool {
    layout.validate().is_ok() && layout.panels().iter().all(|id| registered(id)) && docked(layout).contains(&CANVAS.to_owned())
}

/// craft-ui's checks plus PhotoCraft's registry, for the layout and every hidden panel.
pub(crate) fn valid_workspace(workspace: &Workspace) -> bool {
    workspace.validate().is_ok()
        && valid(&workspace.layout)
        && workspace.saved.as_ref().is_none_or(valid)
        && workspace.hidden.iter().all(|(id, at)| registered(id) && at.anchor.as_deref().is_none_or(registered))
}

/// Read a saved `docking` workspace and where its floating panels dock back. Anything else,
/// such as the bare layouts written before workspaces, gives `None`: the native dock's default.
pub(crate) fn load(saved: &Value) -> Option<(Workspace, Returns)> {
    let docking = saved.get("docking").filter(|v| v.get("layout").is_some())?;
    let mut workspace: Workspace = serde_json::from_value(docking.clone()).ok()?;
    workspace.layout = beside_canvas(workspace.layout);
    workspace.saved = workspace.saved.map(beside_canvas);
    let mut returns: Returns = saved.get("dockingReturns").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    returns.retain(|panel, at| registered(panel) && at.floating.is_none() && floats(&workspace.layout, panel));
    valid_workspace(&workspace).then_some((workspace, returns))
}

/// The saved `docking` workspace is from a newer version than craft-ui reads.
pub(crate) fn newer(saved: &Value) -> bool {
    saved.get("docking").and_then(|docking| docking.get("version")).and_then(Value::as_u64).is_some_and(|version| version > u64::from(Workspace::VERSION))
}

/// A saved workspace keeps its collapsed groups in the layout. Older saves listed the collapsed
/// panels as `dockingCollapsed` and left the native dock's collapse flags on an intact native
/// group; both minimize the group once here, and the flags are cleared.
pub(crate) fn migrate_collapsed(app: &mut PhotocraftApp, saved: &Value) {
    let Some(mut workspace) = app.ui.docking.take() else { return };
    let mut collapsed: std::collections::HashSet<String> =
        saved.get("dockingCollapsed").and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok()).unwrap_or_default().into_iter().collect();
    let pro = app.ui.theme.is_pro();
    for panels in root_groups(&workspace.layout) {
        let group = panels.first().and_then(|id| PANELS.iter().find(|p| p.0 == id)).map(|p| p.2);
        if let Some(group) = group.filter(|group| app.ui.dock.is_collapsed(*group)) {
            let native: Vec<_> = app.ui.dock.visible_tabs(group, pro).iter().filter_map(|(_, name)| normalize(name)).collect();
            if panels.len() == native.len() && panels.iter().all(|panel| native.contains(&panel.as_str())) {
                collapsed.extend(panels.iter().cloned());
            }
        }
    }
    for panels in root_groups(&workspace.layout) {
        if let Some(first) = panels.first().filter(|_| panels.iter().all(|id| collapsed.contains(id))) {
            let _ = workspace.apply(Action::SetMinimized { panel: first.clone(), minimized: true }, permissions);
        }
    }
    app.ui.docking = Some(workspace);
    for group in Group::ALL {
        app.ui.dock.set_collapsed(group, false);
    }
}

fn legacy(app: &PhotocraftApp) -> Layout<String> {
    legacy_layout(app, false)
}

/// The native dock's arrangement as a craft-ui layout: the canvas beside one dock column of the
/// shown groups, top to bottom, with their tab order, selected tab and user-set heights. The
/// Studio themes float Properties over the canvas's top right corner. `all` includes the groups
/// that are not shown, with every tab.
fn legacy_layout(app: &PhotocraftApp, all: bool) -> Layout<String> {
    let pro = app.ui.theme.is_pro();
    let mut groups = Vec::new();
    let mut floating = Vec::new();
    for group in app.ui.dock.order().into_iter().filter(|g| all || g.shown(&app.ui.panels)) {
        let labels = group.tabs(pro);
        let visible = if all { group.tabs(pro).iter().copied().enumerate().collect() } else { app.ui.dock.visible_tabs(group, pro) };
        let panels: Vec<String> = visible.iter().filter_map(|(_, label)| normalize(label).map(str::to_owned)).collect();
        if panels.is_empty() {
            continue;
        }
        let selected = *group.tab_mut(&mut app.ui.dock_tabs.clone());
        let active = labels.get(selected).and_then(|label| normalize(label)).and_then(|id| panels.iter().position(|p| p == id)).unwrap_or(0);
        if group == Group::Properties && !pro {
            floating.push(Floating::single(panels, active, studio_properties_rect(app)).with_id(PROPERTIES_WINDOW));
            continue;
        }
        let height = app.ui.dock.heights.contains_key(&group).then(|| app.ui.dock.height(group));
        groups.push(craft_ui::docking::Group { height, minimized: app.ui.dock.is_collapsed(group), ..craft_ui::docking::Group::new(panels, active) });
    }
    let (columns, width) = if groups.is_empty() { (Vec::new(), 0.0) } else { (vec![Column::new(groups, default_width(pro))], default_width(pro)) };
    Layout { root: Some(with_canvas(Node::Dock { side: Side::End, columns }, width)), floating }
}

/// Where the Studio themes' Properties card floats: inside the canvas's top right corner.
fn studio_properties_rect(app: &PhotocraftApp) -> [f32; 4] {
    let (width, height) = (320.0, 440.0);
    let canvas = app.last_canvas_rect;
    if canvas.is_positive() && canvas.width() > width + 24.0 {
        [canvas.right() - width - 12.0, canvas.top() + 44.0, width, height]
    } else {
        [120.0, 160.0, width, height]
    }
}

/// The native dock as a workspace: its shown panels, and where each hidden one goes back.
fn legacy_workspace(app: &PhotocraftApp) -> Workspace {
    let layout = legacy(app);
    let hidden = migration_hidden(app, &layout);
    Workspace { hidden, ..Workspace::new(layout) }
}

/// The workspace commands change: the customized one, or the native dock's. The lock is the
/// Lock Workspace preference.
fn workspace_of(app: &PhotocraftApp) -> Result<Workspace, String> {
    let mut workspace = match &app.ui.docking {
        Some(workspace) if !valid_workspace(workspace) => return Err("saved panel layout is invalid; reset the workspace first".into()),
        Some(workspace) => workspace.clone(),
        None => legacy_workspace(app),
    };
    workspace.locked = app.session.prefs().workspace_locked;
    Ok(workspace)
}

fn commit(app: &mut PhotocraftApp, workspace: Workspace, mut returns: Returns) {
    returns.retain(|panel, _| floats(&workspace.layout, panel));
    app.ui.docking = Some(workspace);
    app.ui.docking_returns = returns;
    // A group the native dock had collapsed is minimized in the layout from here on.
    for group in Group::ALL {
        app.ui.dock.set_collapsed(group, false);
    }
    app.ui.panels.dock = true;
    sync_visibility(app);
}

fn floats(layout: &Layout<String>, panel: &str) -> bool {
    layout.floating.iter().any(|window| window.contains(&panel.to_owned()))
}

/// The panels of the dock groups that are minimized (PhotoCraft's collapsed groups).
fn collapsed_members(layout: &Layout<String>) -> std::collections::HashSet<String> {
    layout.root.iter().flat_map(tree_groups).filter(|group| group.minimized).flat_map(|group| group.panels).collect()
}

/// The panels of the minimized groups, sorted: `inspect`'s `dockingCollapsed`.
pub(crate) fn collapsed_panels(app: &PhotocraftApp) -> Vec<String> {
    let mut panels: Vec<String> = app.ui.docking.iter().flat_map(|workspace| collapsed_members(&workspace.layout)).collect();
    panels.sort();
    panels
}

fn first_root(layout: &Layout<String>, except: &str) -> Option<String> {
    docked(layout).panels().into_iter().find(|p| p.as_str() != except && p.as_str() != CANVAS).cloned()
}

/// Show `panel` if it is not in the layout: a closed panel at its recorded place, one never laid
/// out where the native dock puts it ([`default_place`]).
fn ensure(app: &PhotocraftApp, workspace: &mut Workspace, panel: &str) -> Result<(), String> {
    if workspace.layout.contains(&panel.to_owned()) {
        return Ok(());
    }
    if workspace.hidden.iter().any(|(id, _)| id == panel) {
        let anchor = first_root(&workspace.layout, panel);
        return workspace.set_visible(panel.to_owned(), true, anchor, permissions).map_err(|e| e.to_string());
    }
    let action = default_place(app, &workspace.layout, panel);
    workspace.apply(action, permissions).map(drop).map_err(|e| e.to_string())
}

/// Where the native dock shows `panel` in `layout`: a tab of its group, before the next of the
/// group's tabs; else a new group of the dock column, above the next native group there or below
/// the one before it; else where craft-ui opens a panel.
fn default_place(app: &PhotocraftApp, layout: &Layout<String>, panel: &str) -> Action<String> {
    let open = Action::Open { panel: panel.to_owned(), anchor: None };
    let Some(group) = PANELS.iter().find(|p| p.0 == panel).map(|p| p.2) else { return open };
    let pro = app.ui.theme.is_pro();
    let tabs: Vec<&str> = app.ui.dock.ordered_tabs(group, pro).iter().filter_map(|(_, label)| normalize(label)).collect();
    if let Some(mate) = tabs.iter().find(|id| layout.contains(&id.to_string())) {
        let members = group_members(layout, mate);
        let after = tabs.iter().position(|id| *id == panel).map_or(&[][..], |at| &tabs[at + 1..]);
        let before = after.iter().find(|id| members.iter().any(|m| m == *id)).map(|id| id.to_string());
        return Action::OpenAt { panel: panel.to_owned(), anchor: mate.to_string(), placement: Placement::Tab { before } };
    }
    let order = app.ui.dock.order();
    let Some(at) = order.iter().position(|g| *g == group) else { return open };
    let docked = |g: &Group| PANELS.iter().filter(|p| p.2 == *g).map(|p| p.0).find(|id| in_dock_column(layout, id));
    let new_group =
        |anchor: &str, below: bool| Action::OpenAt { panel: panel.to_owned(), anchor: anchor.to_owned(), placement: Placement::Group { below, height: None } };
    if let Some(anchor) = order[at + 1..].iter().find_map(docked) {
        new_group(anchor, false)
    } else if let Some(anchor) = order[..at].iter().rev().find_map(docked) {
        new_group(anchor, true)
    } else {
        open
    }
}

/// The panel a command names, if any.
fn command_panel(command: &DockCommand<String>) -> Option<&String> {
    use craft_ui::docking::{DockOperation, UiDock};
    match command {
        DockCommand::Move { panel, .. }
        | DockCommand::Float { panel, .. }
        | DockCommand::Dock { panel, .. }
        | DockCommand::Activate { panel }
        | DockCommand::Close { panel }
        | DockCommand::CloseGroup { panel }
        | DockCommand::CloseOthers { panel }
        | DockCommand::Minimize { panel, .. }
        | DockCommand::Iconic { panel, .. }
        | DockCommand::Window { panel, .. } => Some(panel),
        DockCommand::UiDock(UiDock::Operation(
            DockOperation::Open { panel }
            | DockOperation::Activate { panel }
            | DockOperation::Close { panel }
            | DockOperation::Float { panel, .. }
            | DockOperation::Move { panel, .. },
        )) => Some(panel),
        DockCommand::Layout { action: Action::Activate { panel } } | DockCommand::UiDock(UiDock::Action { action: Action::Activate { panel } }) => Some(panel),
        _ => None,
    }
}

/// Commands that bring their panel to the front.
fn reveals(command: &DockCommand<String>) -> bool {
    use craft_ui::docking::{DockOperation, UiDock};
    matches!(
        command,
        DockCommand::Activate { .. }
            | DockCommand::Layout { action: Action::Activate { .. } }
            | DockCommand::UiDock(
                UiDock::Operation(DockOperation::Open { .. } | DockOperation::Activate { .. }) | UiDock::Action { action: Action::Activate { .. } }
            )
    )
}

/// Commands that close their panel, so a hidden one is not shown first.
fn closes(command: &DockCommand<String>) -> bool {
    use craft_ui::docking::{DockOperation, UiDock};
    matches!(
        command,
        DockCommand::Close { .. }
            | DockCommand::CloseGroup { .. }
            | DockCommand::CloseOthers { .. }
            | DockCommand::UiDock(UiDock::Operation(DockOperation::Close { .. }))
    )
}

pub(crate) fn command(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id == "window.panel.group" {
        return Some(group_command(app, params));
    }
    if let Some(panel) = id.strip_prefix("window.panel.").or_else(|| id.strip_prefix("window.toggle.")).and_then(normalize) {
        if app.ui.docking.is_none() && panel != "brushes" {
            return None;
        }
        let layout = app.ui.docking.as_ref().map(|workspace| &workspace.layout);
        let showing = layout.is_some_and(|layout| layout.contains(&panel.to_string()));
        let show = match params.get("show").or_else(|| params.get("on")) {
            None => !showing || layout.is_some_and(|layout| !panel_active(layout, panel) || collapsed_members(layout).contains(panel)),
            Some(value) => match value.as_bool() {
                Some(show) => show,
                None => return Some(Err("show must be a boolean".into())),
            },
        };
        let id = if show { "window.panel.activate" } else { "window.panel.close" };
        return Some(dock_command(app, id, &json!({"panel": panel})));
    }
    let id = match id {
        "window.panel" if app.ui.docking.is_some() => "window.panel.activate",
        #[cfg(target_arch = "wasm32")]
        "window.panel.window" => return Some(Err("floating panels stay in the page on the web".into())),
        id if DockCommand::<String>::IDS.contains(&id) => id,
        _ => return None,
    };
    Some(dock_command(app, id, params))
}

pub(crate) fn checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    let workspace = app.ui.docking.as_ref()?;
    let panel = id.strip_prefix("window.panel.").or_else(|| id.strip_prefix("window.toggle.")).and_then(normalize)?;
    Some(workspace.layout.contains(&panel.to_string()))
}

/// Parse a docking command id and its params as craft-ui's [`DockCommand`] and run it. Panels
/// may be named by id or label, and `onto` stands for `anchor`.
fn dock_command(app: &mut PhotocraftApp, id: &str, params: &Value) -> Result<Value, String> {
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
                let panel = if canvas { Some(CANVAS) } else { value.as_str().and_then(normalize) };
                let panel = panel.ok_or_else(|| format!("{key} must name a registered panel"))?;
                fields.insert(key.into(), json!(panel));
            }
        }
    }
    fields.insert("command".into(), json!(id));
    let command: DockCommand<String> = serde_json::from_value(Value::Object(fields)).map_err(|error| error.to_string())?;
    run(app, command)
}

/// UI output and automation share this atomic dispatcher: everything applies, or nothing.
fn run(app: &mut PhotocraftApp, command: DockCommand<String>) -> Result<Value, String> {
    if command.is_app_owned() {
        return app_owned(app, command);
    }
    if let DockCommand::Lock { locked } = command {
        let on = locked.unwrap_or(!app.session.prefs().workspace_locked);
        app.session.prefs.edit(|p| p.workspace_locked = on);
        if let Some(workspace) = app.ui.docking.as_mut() {
            workspace.locked = on;
        }
        return Ok(json!({"locked": on}));
    }
    let mut workspace = workspace_of(app)?;
    let mut returns = app.ui.docking_returns.clone();
    let panel = command_panel(&command).cloned();
    // A hidden panel a command moves is shown first, as the Window menu shows it.
    if let Some(panel) = panel.as_ref().filter(|_| !closes(&command)) {
        ensure(app, &mut workspace, panel)?;
    }
    let space = FloatSpace::new(FLOAT_AREA);
    let rtl = crate::direction::is_rtl();
    let closed = match &command {
        DockCommand::Dock { panel, anchor: None, side: None, zone: None, group: false, column: false, .. } if dock_back(&mut workspace, &returns, panel)? => {
            Vec::new()
        }
        DockCommand::Float { panel, x, y, width, height, group: false, column: false } => {
            if let Ok(location) = workspace.layout.location(panel)
                && location.floating.is_none()
            {
                returns.insert(panel.clone(), location);
            }
            let sized = DockCommand::Float {
                panel: panel.clone(),
                x: *x,
                y: *y,
                width: width.or(Some(FLOAT_SIZE.x)),
                height: height.or(Some(FLOAT_SIZE.y)),
                group: false,
                column: false,
            };
            sized.apply(&mut workspace, rtl, &space, permissions).map_err(|e| e.to_string())?
        }
        DockCommand::Window { panel, native } if native.unwrap_or(true) => {
            if workspace.layout.floating.iter().any(|window| window.id == PROPERTIES_WINDOW && window.contains(panel)) {
                return Err("the Properties card stays over the canvas; float the panel out of it first".into());
            }
            if let Ok(location) = workspace.layout.location(panel)
                && location.floating.is_none()
            {
                returns.insert(panel.clone(), location);
            }
            command.apply(&mut workspace, rtl, &space, permissions).map_err(|e| e.to_string())?
        }
        command => command.apply(&mut workspace, rtl, &space, permissions).map_err(|e| e.to_string())?,
    };
    if !valid_workspace(&workspace) {
        return Err("docking action contains an unknown panel".into());
    }
    let reveal = panel.filter(|_| reveals(&command));
    if let Some(panel) = &reveal
        && workspace.layout.panel_state(panel) != PanelState::Front
    {
        // A minimized window opens again; where nothing is minimized the action is refused.
        let _ = workspace.apply(Action::SetMinimized { panel: panel.clone(), minimized: false }, permissions);
    }
    commit(app, workspace, returns);
    if let Some(panel) = &reveal {
        app.ui.docking_reveal = Some(panel.clone());
    }
    let layout = app.ui.docking.as_ref().map(|workspace| &workspace.layout);
    Ok(json!({"panel": command_panel(&command), "layout": layout, "closed": closed}))
}

/// Return a floating panel to where it floated from, if that place is still in the dock.
fn dock_back(workspace: &mut Workspace, returns: &Returns, panel: &String) -> Result<bool, String> {
    let Some(location) = returns.get(panel).filter(|_| floats(&workspace.layout, panel)) else { return Ok(false) };
    if workspace.locked {
        return Err("workspace is locked".into());
    }
    let docked = Layout { root: workspace.layout.root.clone(), floating: Vec::new() };
    if location.anchor.as_ref().is_some_and(|anchor| !docked.contains(anchor)) {
        return Ok(false);
    }
    let mut layout = workspace.layout.clone();
    layout.apply(Action::Close { panel: panel.clone() }).map_err(|e| e.to_string())?;
    if layout.restore(panel.clone(), location).is_err() || !valid(&layout) {
        return Ok(false);
    }
    workspace.layout = layout;
    Ok(true)
}

/// Named workspaces and Reset are PhotoCraft's (Window › Workspace).
fn app_owned(app: &mut PhotocraftApp, command: DockCommand<String>) -> Result<Value, String> {
    let (id, params) = match command {
        DockCommand::Workspace { name } => ("window.workspace.select", json!({"name": name})),
        DockCommand::NewWorkspace { name } => ("window.workspace.newWorkspace", json!({"name": name})),
        DockCommand::DeleteWorkspace { name } => ("window.workspace.deleteWorkspace", json!({"name": name})),
        DockCommand::ResetWorkspace {} => {
            crate::menus::apply_workspace(app);
            return Ok(json!({"workspace": app.ui.workspace}));
        }
        _ => return Err("not a workspace command".into()),
    };
    crate::workspace_ui::menu(app, id, &params).unwrap_or_else(|| Err(format!("`{id}` is not available")))
}

/// Draw the layout, the canvas included, in the space `ui` has left: the customized workspace,
/// else the native dock's arrangement.
pub(crate) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let area = area_id();
    let changed_workspace = ui.ctx().data_mut(|data| {
        let previous = data.get_temp::<u64>(area.with("generation"));
        data.insert_temp(area.with("generation"), app.ui.docking_generation);
        previous.is_some_and(|generation| generation != app.ui.docking_generation)
    });
    if changed_workspace {
        craft_ui::docking::cancel_drag::<String>(ui.ctx(), area);
    }
    if app.ui.docking.as_ref().is_some_and(|workspace| !valid_workspace(workspace)) {
        app.ui.docking = None;
        app.ui.docking_returns.clear();
        let message = "Invalid panel layout; restored the default dock";
        app.ui.status = tl!(message).into();
    }
    if let Some(width) = ui.ctx().data_mut(|data| data.remove_temp::<f32>(crate::panels::dock_width_id())) {
        let range = crate::panels::dock_width_range(ui.ctx().content_rect().width());
        set_dock_width(app, ui, width.clamp(*range.start(), *range.end()));
    }
    native_dock_width(app, ui);
    let layout = match &app.ui.docking {
        Some(workspace) => workspace.layout.clone(),
        None => legacy(app),
    };
    let collapsed = collapsed_members(&layout);
    let fillers = fillers(&layout);
    let reveal = app.ui.docking_reveal.take();
    let t = crate::theme::Tokens::get(ui.ctx());
    let locked = app.session.prefs().workspace_locked;
    let mut style = craft_ui::docking::DockStyle::from_ui(ui);
    style.tab_height = if t.pro { 28.0 } else { 40.0 };
    style.min_pane = 0.0;
    style.background = t.card;
    // Studio themes leave the strip transparent over the dock's card; a floating group has no
    // card behind it, so the canvas showed through its titles (#4509).
    style.tab_background = if t.tab_strip.a() == 0 { t.card } else { t.tab_strip };
    style.active_background = t.card;
    style.text = t.text;
    style.inactive_text = t.text_dim;
    style.border = craft_ui::theme::scale::Stroke::W1.egui(ui.ctx(), t.card_border);
    style.accent = t.accent;
    style.right_to_left = crate::direction::is_rtl();
    style.new_dock_zones = true;
    // The panel menu button ends every tab strip; its menu has the panel's items, then the group's
    // (collapse, float, move up and down, close).
    style.panel_menu_button = true;
    let style = style.localized(|key| DOCK_LABELS.iter().find(|(k, _)| *k == key).map(|(_, english)| tl!(english).to_string()));
    let mut dock = craft_ui::docking::DockArea::new(area)
        .locked(locked)
        .auto_collapse_iconic(app.session.prefs().workspace.auto_collapse_icon_panels)
        .input_blocked(modal_open(app, ui.ctx()));
    if let Some(panel) = &reveal {
        dock = dock.raise_panel(panel.clone());
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        dock = dock.native_input_hook(native_input_hook(ui.ctx()));
    }
    let strip = style.tab_height;
    let layers_min = crate::panels::layer_row::min_panel_width(ui, app.ui.layers_panel_options.thumbnail_size);
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(bodies_id().with("layers-min"), layers_min.max(220.0)));
    let card: std::collections::HashSet<String> =
        layout.floating.iter().filter(|window| window.id == PROPERTIES_WINDOW).flat_map(|window| window.panels().cloned()).collect();
    #[cfg(test)]
    ui.ctx().data_mut(|data| data.insert_temp(bodies_id(), Vec::<(String, egui::Rect)>::new()));
    // The panel windows' passes, inside this call, note their keyboard focus again.
    keyboard_focus(ui.ctx()).lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut output = dock.show_customized(
        ui,
        &layout,
        &style,
        |id| PANELS.iter().find(|p| p.0 == id).map(|p| tl!(p.1).to_string()).unwrap_or_else(|| id.clone()),
        permissions,
        |id| {
            if id == CANVAS {
                craft_ui::docking::PanelLimits { min: egui::vec2(200.0, 120.0), ..Default::default() }
            } else if collapsed.contains(id) {
                craft_ui::docking::PanelLimits { min: egui::vec2(220.0, 0.0), max: egui::vec2(1_000_000.0, 0.0) }
            } else {
                // The native group's smallest height, tab strip included.
                let height = PANELS.iter().find(|p| p.0 == id).map_or(80.0, |p| p.2.min_height() - strip);
                craft_ui::docking::PanelLimits { min: egui::vec2(panel_min_width(id, layers_min), height), ..Default::default() }
            }
        },
        &mut PanelContent { app, collapsed: &collapsed, fillers: &fillers, card: &card, strip },
    );
    if let Some(error) = output.error.take() {
        app.ui.status = error.to_string();
    }
    #[cfg(not(target_arch = "wasm32"))]
    open_window_drops(app, ui.ctx(), std::mem::take(&mut output.window_files));
    // Keys pressed in a floating panel's own OS window run the main window's shortcuts.
    let forwarded = std::mem::take(&mut output.forwarded_events);
    crate::shortcuts::handle_forwarded(app, ui.ctx(), forwarded, output.forwarded_modifiers.unwrap_or_default());
    let mut actions = output.actions;
    pin_group_heights(ui.ctx(), &layout, &mut actions);
    for action in actions {
        if let Err(error) = app.run("window.panel.layout", json!({"action": action})) {
            app.ui.status = error;
        }
    }
}

/// A modal dialog or workspace is open. egui's `Modal` blocks input only in the window it shows
/// in, so the floating panels' own OS windows are blocked meanwhile (`DockArea::input_blocked`).
/// The app's dialogs count from the frame they open; any other egui modal (a format's options,
/// a job's progress) from the frame after it first shows.
pub(crate) fn modal_open(app: &PhotocraftApp, ctx: &egui::Context) -> bool {
    !app.ui.dialogs.is_empty()
        || app.discard.is_some()
        || app.camera_raw.is_some()
        || crate::distort_ui::modal_workspace(app).is_some()
        || !ctx.memory(|memory| memory.is_above_modal_layer(egui::LayerId::background()))
}

/// The viewports of the floating windows that may have OS windows of their own. Where egui
/// embeds viewports, and on the web, every floating window stays in the main window.
pub(crate) fn native_viewports(app: &PhotocraftApp, ctx: &egui::Context) -> Vec<egui::ViewportId> {
    #[cfg(not(target_arch = "wasm32"))]
    if !ctx.embed_viewports()
        && let Some(workspace) = &app.ui.docking
    {
        return workspace
            .layout
            .floating
            .iter()
            .filter(|window| window.id != PROPERTIES_WINDOW)
            .map(|window| craft_ui::docking::DockArea::<String>::window_viewport(area_id(), window.id))
            .collect();
    }
    let _ = (app, ctx);
    Vec::new()
}

/// The main window's `raw_input_hook` rewrites that floating panels' own windows need too:
/// Ctrl+click is a secondary click on macOS. Each window keeps its own press state.
#[cfg(not(target_arch = "wasm32"))]
fn native_input_hook(ctx: &egui::Context) -> impl FnMut(&egui::Context, &mut egui::RawInput) + Send + 'static {
    type Clicks = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<egui::ViewportId, crate::control_click::ControlClick>>>;
    let clicks = ctx.data_mut(|data| data.get_temp_mut_or_default::<Clicks>(area_id().with("control-click")).clone());
    move |_, raw| {
        let mut clicks = clicks.lock().unwrap_or_else(|e| e.into_inner());
        clicks.entry(raw.viewport_id).or_default().normalize(&mut raw.events, cfg!(target_os = "macos"));
    }
}

/// The floating panels' own windows where a widget had keyboard focus in their last pass.
type KeyboardFocus = std::sync::Arc<std::sync::Mutex<std::collections::HashSet<egui::ViewportId>>>;

fn keyboard_focus(ctx: &egui::Context) -> KeyboardFocus {
    ctx.data_mut(|data| data.get_temp_mut_or_default::<KeyboardFocus>(area_id().with("keyboard-focus")).clone())
}

/// Another of the app's windows has the keyboard, by this frame's viewport infos: `Some(true)`
/// when it is a floating panel's own window and one of its widgets has keyboard focus.
pub(crate) fn other_window_keyboard(ctx: &egui::Context) -> Option<bool> {
    let window = ctx.input(|i| i.raw.viewports.iter().find(|(id, info)| **id != egui::ViewportId::ROOT && info.focused == Some(true)).map(|(id, _)| *id))?;
    Some(keyboard_focus(ctx).lock().unwrap_or_else(|e| e.into_inner()).contains(&window))
}

/// Files dropped on a floating panel's own OS window open as documents, as files dropped on the
/// main window away from the canvas and the tab strip do, and the main window comes forward.
#[cfg(not(target_arch = "wasm32"))]
fn open_window_drops(app: &mut PhotocraftApp, ctx: &egui::Context, windows: Vec<craft_ui::docking::WindowFiles<String>>) {
    let dropped: Vec<egui::DroppedFileHandle> = windows.into_iter().flat_map(|window| window.dropped).collect();
    if !dropped.is_empty() {
        app.open_dropped(ctx, dropped, None);
        ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
    }
}

/// A divider drag pins the other open groups of its column at the heights they show, so groups
/// still at their defaults, which give way to the filler, don't shift while one is resized. The
/// last open group fills the column and stays unpinned.
fn pin_group_heights(ctx: &egui::Context, layout: &Layout<String>, actions: &mut Vec<Action<String>>) {
    use craft_ui::docking::{DockArea, Hit};
    let resized: Vec<String> = actions.iter().filter_map(|a| if let Action::ResizeGroup { panel, .. } = a { Some(panel.clone()) } else { None }).collect();
    if resized.is_empty() {
        return;
    }
    let hits = DockArea::<String>::hit_rects(ctx, area_id());
    let mut pins = Vec::new();
    let mut pending: Vec<&Node<String>> = layout.root.iter().collect();
    while let Some(node) = pending.pop() {
        match node {
            Node::Split { first, second, .. } => pending.extend([first.as_ref(), second.as_ref()]),
            Node::Dock { columns, .. } => {
                for column in columns.iter().filter(|c| c.groups.iter().any(|g| g.panels.first().is_some_and(|p| resized.contains(p)))) {
                    let open: Vec<_> = column.groups.iter().filter(|g| !g.minimized).collect();
                    for group in open.iter().take(open.len().saturating_sub(1)).filter(|g| g.height.is_none()) {
                        let Some(first) = group.panels.first().filter(|p| !resized.contains(p)) else { continue };
                        let top = hits
                            .iter()
                            .filter(|(hit, _)| matches!(hit, Hit::Tab(p) | Hit::DragBar(p) if group.panels.contains(p)))
                            .map(|(_, rect)| rect.top())
                            .reduce(f32::min);
                        let bottom =
                            hits.iter().find(|(hit, _)| *hit == Hit::Divider(craft_ui::docking::Divider::Group(first.clone()))).map(|(_, r)| r.center().y);
                        if let (Some(top), Some(bottom)) = (top, bottom) {
                            pins.push(Action::ResizeGroup { panel: first.clone(), height: Some(bottom - top) });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    actions.splice(0..0, pins);
}

/// The narrowest width panel `id` allows its column (points); the Layers panel's depends on the
/// theme's row.
fn panel_min_width(id: &str, layers_min: f32) -> f32 {
    if id == "layers" { layers_min.max(220.0) } else { 220.0 }
}

/// `ui.set {dockWidth}`: the width of the end dock's outer column, at least what its panels'
/// limits allow.
fn set_dock_width(app: &mut PhotocraftApp, ui: &egui::Ui, width: f32) {
    let Ok(mut workspace) = workspace_of(app) else { return };
    let outer = match workspace.layout.root.as_ref() {
        Some(Node::Split { second, .. }) => match second.as_ref() {
            Node::Dock { columns, .. } => columns.first().map(|column| (column.width, column.groups.iter().flat_map(|g| g.panels.first()).next().cloned())),
            _ => None,
        },
        _ => None,
    };
    let Some((current, Some(panel))) = outer else { return };
    let layers_min = crate::panels::layer_row::min_panel_width(ui, app.ui.layers_panel_options.thumbnail_size);
    let least = outer_column_panels(&workspace).iter().map(|id| panel_min_width(id, layers_min)).fold(0.0, f32::max);
    let width = width.max(least);
    if current == width {
        return;
    }
    workspace.locked = false;
    if workspace.apply(Action::ResizeColumn { panel, width }, permissions).is_ok() {
        workspace.locked = app.session.prefs().workspace_locked;
        let returns = app.ui.docking_returns.clone();
        commit(app, workspace, returns);
    }
}

/// A dock width saved by earlier versions is in egui's remembered memory under the `dock` side
/// panel's id. The default layout's column takes it once, after the canvas has been laid out so
/// the Studio Properties card keeps its place.
fn native_dock_width(app: &mut PhotocraftApp, ui: &egui::Ui) {
    let ctx = ui.ctx();
    let id = crate::panels::panel_state_id(ctx, "dock");
    let Some(state) = ctx.data_mut(|data| data.get_persisted::<egui::PanelState>(id)) else { return };
    if app.ui.docking.is_none() && !app.last_canvas_rect.is_positive() {
        return;
    }
    ctx.data_mut(|data| data.remove::<egui::PanelState>(id));
    let pro = app.ui.theme.is_pro();
    let range = crate::panels::dock_width_range(ctx.content_rect().width());
    let width = state.size().x;
    if app.ui.docking.is_none() && width.is_finite() && (width - default_width(pro)).abs() >= 1.0 {
        set_dock_width(app, ui, width.clamp(*range.start(), *range.end()));
    }
}

/// Every panel of the end dock's outer column.
fn outer_column_panels(workspace: &Workspace) -> Vec<String> {
    match workspace.layout.root.as_ref() {
        Some(Node::Split { second, .. }) => match second.as_ref() {
            Node::Dock { columns, .. } => columns.first().into_iter().flat_map(|c| c.groups.iter().flat_map(|g| g.panels.iter().cloned())).collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The panels of each dock column's last open group, which fills the column.
fn fillers(layout: &Layout<String>) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let mut pending: Vec<&Node<String>> = layout.root.iter().collect();
    while let Some(node) = pending.pop() {
        match node {
            Node::Split { first, second, .. } => pending.extend([first.as_ref(), second.as_ref()]),
            Node::Dock { columns, .. } => {
                for column in columns {
                    out.extend(column.groups.iter().rev().find(|g| !g.minimized).into_iter().flat_map(|g| g.panels.iter().cloned()));
                }
            }
            _ => {}
        }
    }
    out
}

fn body(app: &mut PhotocraftApp, ui: &mut egui::Ui, id: &str) {
    let Some((_, label, group)) = PANELS.iter().find(|p| p.0 == id) else { return };
    let pro = crate::theme::Tokens::get(ui.ctx()).pro;
    let Some(tab) = group.tabs(pro).iter().position(|name| name == label) else { return };
    // The native group's selected tab is this panel while its body draws, so a custom group
    // holding several of the group's panels keeps the selection `sync_visibility` chose.
    let shown = std::mem::replace(group.tab_mut(&mut app.ui.dock_tabs), tab);
    if group.scrolls_itself(tab) {
        ui.set_min_height(ui.available_height().max(0.0));
        crate::panels::dock_body(app, ui, *group, tab);
    } else {
        crate::panels::offer_color_field_fill(ui, pro, *group, tab);
        // No scroll fade: it darkens real colours, swatches and previews at the edge (#2726).
        ui.spacing_mut().scroll.fade.strength = 0.0;
        egui::ScrollArea::vertical()
            .id_salt(("shared-panel-body", id))
            .auto_shrink([false, false])
            .show(ui, |ui| crate::panels::dock_body(app, ui, *group, tab));
    }
    let selected = *group.tab_mut(&mut app.ui.dock_tabs);
    if selected == tab {
        *group.tab_mut(&mut app.ui.dock_tabs) = shown;
    } else if let Some(label) = group.tabs(pro).get(selected) {
        let _ = app.run("window.panel.activate", json!({"panel":label}));
    }
}

/// Where tests find panels and docking controls as drawn last frame.
#[cfg(test)]
pub(crate) mod probe {
    use super::*;
    use craft_ui::docking::{DockArea, Hit};
    use egui::Rect;

    pub(crate) fn hit_rect(ctx: &egui::Context, hit: &Hit<String>) -> Option<Rect> {
        DockArea::<String>::hit_rects(ctx, area_id()).into_iter().find(|(drawn, _)| drawn == hit).map(|(_, rect)| rect)
    }

    pub(crate) fn tab_rect(ctx: &egui::Context, panel: &str) -> Option<Rect> {
        hit_rect(ctx, &Hit::Tab(panel.to_owned()))
    }

    /// The body of `panel`, when it is its group's selected tab.
    pub(crate) fn panel_rect(ctx: &egui::Context, panel: &str) -> Option<Rect> {
        last_bodies(ctx).into_iter().find(|(id, _)| id == panel).map(|(_, rect)| rect)
    }

    /// The panels of the tab group holding `panel`, in tab order.
    pub(crate) fn group_of(app: &PhotocraftApp, panel: &str) -> Vec<String> {
        let layout = app.ui.docking.as_ref().map_or_else(|| legacy(app), |workspace| workspace.layout.clone());
        group_members(&layout, panel)
    }

    /// The tab group holding `panel`: its tab strip and its selected panel's body.
    pub(crate) fn group_rect(app: &PhotocraftApp, ctx: &egui::Context, panel: &str) -> Option<Rect> {
        let members = group_of(app, panel);
        let strip = DockArea::<String>::hit_rects(ctx, area_id()).into_iter().filter_map(|(hit, rect)| match hit {
            Hit::Tab(p) | Hit::DragBar(p) | Hit::Border(p) | Hit::Overflow(p) => members.contains(&p).then_some(rect),
            _ => None,
        });
        let bodies = last_bodies(ctx).into_iter().filter_map(|(id, rect)| members.contains(&id).then_some(rect));
        strip.chain(bodies).reduce(|a, b| a.union(b))
    }

    /// [`group_rect`] of the first panel of the native `group` that is laid out.
    pub(crate) fn native_group_rect(app: &PhotocraftApp, ctx: &egui::Context, group: Group) -> Option<Rect> {
        PANELS.iter().filter(|p| p.2 == group).find_map(|p| group_rect(app, ctx, p.0))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod native_tests;

/// Return positions for panels already hidden in a legacy workspace.
fn migration_hidden(app: &PhotocraftApp, visible: &Layout<String>) -> Vec<(String, Location<String>)> {
    let full = legacy_layout(app, true);
    PANELS
        .iter()
        .filter(|(id, _, _)| !visible.contains(&id.to_string()))
        .filter_map(|(id, _, _)| full.location(&id.to_string()).ok().map(|at| ((*id).to_owned(), at)))
        .collect()
}

/// A tab group of a layout tree.
struct TabGroup {
    panels: Vec<String>,
    active: usize,
    /// In a dock column, where craft-ui minimizes it to its tab strip.
    dock: bool,
    minimized: bool,
}

/// The tab groups of a tree, top to bottom: frames, each accordion entry, and each dock
/// column's groups.
fn tree_groups(root: &Node<String>) -> Vec<TabGroup> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        match node {
            Node::Split { first, second, .. } => pending.extend([second.as_ref(), first.as_ref()]),
            Node::Tabs { panels, active } => out.push(TabGroup { panels: panels.clone(), active: *active, dock: false, minimized: false }),
            Node::Stack { entries } => out.extend(entries.iter().map(|e| TabGroup { panels: vec![e.panel.clone()], active: 0, dock: false, minimized: false })),
            Node::Dock { columns, .. } => out.extend(columns.iter().flat_map(|c| &c.groups).map(|g| TabGroup {
                panels: g.panels.clone(),
                active: g.active,
                dock: true,
                minimized: g.minimized,
            })),
            _ => {}
        }
    }
    out
}

/// The tab group holding `panel` in a tree; an accordion entry is a group of its own.
fn tree_group(root: &Node<String>, panel: &str) -> Option<Vec<String>> {
    tree_groups(root).into_iter().find(|group| group.panels.iter().any(|id| id == panel)).map(|group| group.panels)
}

/// Is `panel` in a group of a docked dock column?
fn in_dock_column(layout: &Layout<String>, panel: &str) -> bool {
    layout.root.iter().flat_map(tree_groups).any(|group| group.dock && group.panels.iter().any(|id| id == panel))
}

fn group_members(layout: &Layout<String>, panel: &str) -> Vec<String> {
    layout.root.iter().chain(layout.floating.iter().map(|window| &window.root)).find_map(|root| tree_group(root, panel)).unwrap_or_default()
}

pub(crate) fn rail_on(app: &PhotocraftApp, group: Group) -> bool {
    app.ui.docking.as_ref().is_some_and(|workspace| {
        let layout = &workspace.layout;
        let collapsed = collapsed_members(layout);
        PANELS.iter().filter(|p| p.2 == group).any(|p| layout.contains(&p.0.to_owned()) && !collapsed.contains(p.0))
    })
}

pub(crate) fn rail_click(app: &mut PhotocraftApp, group: Group) {
    let Some(workspace) = app.ui.docking.as_ref() else { return };
    let layout = &workspace.layout;
    let floating = PANELS.iter().filter(|p| p.2 == group).any(|p| floats(layout, p.0));
    let existing = PANELS.iter().find(|p| p.2 == group && layout.contains(&p.0.to_owned())).map(|p| p.0);
    let panel = existing.or_else(|| group.tabs(app.ui.theme.is_pro()).get(*group.tab_mut(&mut app.ui.dock_tabs)).and_then(|label| normalize(label)));
    if let Some(panel) = panel {
        let result = if existing.is_some() && rail_on(app, group) {
            if floating {
                group_command(app, &json!({"panel": panel, "operation":"close"}))
            } else {
                group_command(app, &json!({"panel": panel, "operation":"collapse"}))
            }
        } else {
            dock_command(app, "window.panel.activate", &json!({"panel":panel}))
        };
        if let Err(error) = result {
            app.ui.status = error;
        }
    }
}

/// The docked tab groups, top to bottom, without the canvas.
fn root_groups(layout: &Layout<String>) -> Vec<Vec<String>> {
    layout.root.iter().flat_map(tree_groups).map(|group| group.panels).filter(|panels| !panels.iter().any(|id| id == CANVAS)).collect()
}

/// `window.panel.group {panel, operation}`: automation's commands for a panel's whole group.
/// Collapse and expand are `window.panel.minimize`, up and down `window.panel.stepGroup`.
fn group_command(app: &mut PhotocraftApp, params: &Value) -> Result<Value, String> {
    let panel = params.get("panel").and_then(Value::as_str).and_then(normalize).ok_or("panel must name a registered panel")?;
    let operation = params.get("operation").and_then(Value::as_str).ok_or("operation is required")?;
    if app.session.prefs().workspace_locked {
        return Err("workspace is locked".into());
    }
    let mut workspace = workspace_of(app)?;
    if group_members(&workspace.layout, panel).is_empty() {
        return Err("panel is hidden".into());
    }
    let panel = panel.to_owned();
    match operation {
        "close" => drop(workspace.apply(Action::CloseGroup { panel }, permissions).map_err(|e| e.to_string())?),
        "collapse" | "expand" => {
            drop(workspace.apply(Action::SetMinimized { panel, minimized: operation == "collapse" }, permissions).map_err(|e| e.to_string())?)
        }
        "up" | "down" => drop(workspace.apply(Action::StepGroup { panel, up: operation == "up" }, permissions).map_err(|e| e.to_string())?),
        "move" => {
            let members = group_members(&workspace.layout, &panel);
            if params.get("anchor").and_then(Value::as_str).and_then(normalize).is_some_and(|anchor| members.iter().any(|id| id == anchor)) {
                return Err("group cannot be moved into itself".into());
            }
            let mut params = params.clone();
            params["group"] = json!(true);
            if let Some(fields) = params.as_object_mut() {
                fields.remove("operation");
            }
            let before = (app.ui.docking.clone(), app.ui.docking_returns.clone());
            dock_command(app, "window.panel.move", &params)?;
            workspace = workspace_of(app)?;
            if let Err(error) = workspace.apply(Action::Activate { panel }, permissions) {
                (app.ui.docking, app.ui.docking_returns) = before;
                sync_visibility(app);
                return Err(error.to_string());
            }
        }
        _ => return Err("operation must be close, collapse, expand, move, up or down".into()),
    }
    let returns = app.ui.docking_returns.clone();
    commit(app, workspace, returns);
    Ok(json!({"layout": app.ui.docking.as_ref().map(|workspace| &workspace.layout)}))
}

pub(crate) struct LegacyUpdate {
    workspace: Workspace,
}

/// Validate and stage the legacy controls before ui.set commits any fields.
pub(crate) fn prepare_legacy_update(
    app: &PhotocraftApp,
    request: &Value,
    panels: Option<&crate::state::Panels>,
    tabs: Option<&crate::state::DockTabs>,
    dock: Option<&crate::dock::DockLayout>,
    pro: bool,
) -> Result<Option<LegacyUpdate>, String> {
    let Some(current) = &app.ui.docking else { return Ok(None) };
    if panels.is_none() && tabs.is_none() && dock.is_none() {
        return Ok(None);
    }
    if !valid_workspace(current) {
        return Err("invalid panel layout; reset the workspace first".into());
    }
    // A legacy flat-column replacement cannot represent arbitrary custom splits. Reject it
    // explicitly instead of accepting a command whose requested order/height is ignored.
    if let Some(dock) = dock
        && (dock.order != app.ui.dock.order || dock.heights != app.ui.dock.heights)
    {
        return Err("dock order/heights cannot replace a customized layout; use window.panel.layout".into());
    }
    if app.session.prefs().workspace_locked {
        return Err("workspace is locked".into());
    }
    let collapse_changes: Vec<_> = dock
        .map(|d| Group::ALL.into_iter().filter(|g| d.is_collapsed(*g) != app.ui.dock.is_collapsed(*g)).map(|g| (g, d.is_collapsed(g))).collect())
        .unwrap_or_default();
    if !collapse_changes.is_empty()
        && root_groups(&current.layout).iter().any(|ids| {
            let groups: std::collections::HashSet<_> = ids.iter().filter_map(|id| PANELS.iter().find(|p| p.0 == id).map(|p| p.2)).collect();
            groups.len() > 1
        })
    {
        return Err("legacy collapse settings cannot replace mixed custom groups; use window.panel.group".into());
    }
    let mut workspace = current.clone();
    workspace.locked = false;
    for group in Group::ALL {
        let visibility = request.get("panels").and_then(|p| p.get(group.key())).and_then(Value::as_bool);
        let selected = request.get("dockTabs").and_then(|p| p.get(group.key())).is_some();
        let group_panels: Vec<_> = PANELS.iter().filter(|p| p.2 == group).collect();
        for &&(id, label, _) in &group_panels {
            let explicitly_hidden = dock.is_some_and(|d| d.hidden_tabs.get(&group).is_some_and(|names| names.iter().any(|name| name == label)));
            let explicitly_shown = dock.is_some() && !explicitly_hidden;
            if visibility == Some(false) || explicitly_hidden {
                workspace.set_visible(id.to_owned(), false, None, permissions).map_err(|e| e.to_string())?;
            } else if visibility == Some(true) || explicitly_shown {
                ensure(app, &mut workspace, id)?;
            }
        }
        if selected {
            let tabs = tabs.ok_or("dockTabs is missing")?;
            let index = match group {
                Group::Color => tabs.color,
                Group::Properties => tabs.properties,
                Group::Character => tabs.character,
                Group::Navigator => tabs.navigator,
                Group::History => tabs.history,
                Group::Layers => tabs.layers,
                Group::Brushes => tabs.brushes,
            };
            let id = group.tabs(pro).get(index).and_then(|label| normalize(label)).ok_or("dockTabs index is outside the panel group")?;
            if visibility == Some(false) {
                return Err("cannot select a tab in a hidden group".into());
            }
            ensure(app, &mut workspace, id)?;
            workspace.apply(Action::Activate { panel: id.to_owned() }, permissions).map_err(|e| e.to_string())?;
        }
    }
    // A group the request collapses or expands, by its first panel in the layout; showing a group
    // or selecting its tab expands it.
    for group in Group::ALL {
        let expands = request.get("dockTabs").and_then(|p| p.get(group.key())).is_some()
            || request.get("panels").and_then(|p| p.get(group.key())).and_then(Value::as_bool) == Some(true);
        let minimized = if expands {
            false
        } else if let Some((_, on)) = collapse_changes.iter().find(|(g, _)| *g == group) {
            *on
        } else {
            continue;
        };
        if let Some(panel) = PANELS.iter().find(|p| p.2 == group && workspace.layout.contains(&p.0.to_owned())).map(|p| p.0)
            && collapsed_members(&workspace.layout).contains(panel) != minimized
        {
            workspace.apply(Action::SetMinimized { panel: panel.to_owned(), minimized }, permissions).map_err(|e| e.to_string())?;
        }
    }
    Ok(Some(LegacyUpdate { workspace }))
}

pub(crate) fn commit_legacy_update(app: &mut PhotocraftApp, update: LegacyUpdate, request: &Value) {
    let returns = app.ui.docking_returns.clone();
    commit(app, update.workspace, returns);
    for group in Group::ALL {
        if request.get("dockTabs").and_then(|p| p.get(group.key())).is_some()
            || request.get("panels").and_then(|p| p.get(group.key())).and_then(Value::as_bool) == Some(true)
        {
            app.ui.dock.set_collapsed(group, false);
        }
    }
}

struct PanelContent<'a> {
    app: &'a mut PhotocraftApp,
    collapsed: &'a std::collections::HashSet<String>,
    fillers: &'a std::collections::HashSet<String>,
    /// The panels of the Studio themes' Properties card, which stays over the canvas.
    card: &'a std::collections::HashSet<String>,
    /// The tab strip's height, which the native groups' heights include.
    strip: f32,
}

fn panel_menu(app: &mut PhotocraftApp, ui: &mut egui::Ui, panel: &str) {
    match panel {
        "layers" => {
            crate::layer_row_ui::panel_menu(app, ui);
            ui.separator();
        }
        "swatches" => {
            crate::swatches_ui::panel_menu(app, ui);
            ui.separator();
        }
        "color" => {
            crate::color_panel_ui::panel_menu(app, ui);
            ui.separator();
        }
        "history" => {
            crate::history_menu_ui::panel_menu(app, ui);
            ui.separator();
        }
        "brushSettings" | "brush_settings" => {
            crate::brush_panel::panel_menu(app, ui);
            ui.separator();
        }
        "actions" | "Actions" => {
            crate::actions::panel_menu(app, ui);
            ui.separator();
        }
        "histogram" => {
            crate::histogram_panel::panel_menu(app, ui);
            ui.separator();
        }
        _ => {}
    }
}

impl craft_ui::docking::DockContent<String> for PanelContent<'_> {
    fn body(&mut self, ui: &mut egui::Ui, id: &String) {
        #[cfg(test)]
        ui.ctx().data_mut(|data| data.get_temp_mut_or_default::<Vec<(String, egui::Rect)>>(bodies_id()).push((id.clone(), ui.max_rect())));
        if id == CANVAS {
            crate::panels::canvas_panel(self.app, ui);
            return;
        }
        let viewport = ui.ctx().viewport_id();
        if viewport != egui::ViewportId::ROOT && ui.memory(|m| m.focused().is_some()) {
            keyboard_focus(ui.ctx()).lock().unwrap_or_else(|e| e.into_inner()).insert(viewport);
        }
        // A window too small for the dock squeezes its column below the panels' minimum width,
        // which their layouts do not take.
        if !self.collapsed.contains(id) && ui.available_width() >= 200.0 {
            let pro = crate::theme::Tokens::get(ui.ctx()).pro;
            ui.spacing_mut().item_spacing.y = if pro { 0.0 } else { 6.0 };
            egui::Frame::NONE.inner_margin(crate::widgets::body_margin(pro)).show(ui, |ui| body(self.app, ui, id));
        }
    }
    fn dock_heights(&self, id: &String) -> Option<DockHeights> {
        let group = PANELS.iter().find(|p| p.0 == id)?.2;
        let body = |height: f32| (height - self.strip).max(0.0);
        let fill = if self.fillers.contains(id) { body(group.preferred_fill()).max(1.0) } else { 0.0 };
        Some(DockHeights::new(body(group.default_height())).compact(body(group.compact_height())).fill(fill))
    }
    fn panel_menu(&mut self, ui: &mut egui::Ui, id: &String) {
        panel_menu(self.app, ui, id);
    }
    fn allow_native(&self, id: &String) -> bool {
        !self.card.contains(id)
    }
}

pub(crate) fn sync_visibility(app: &mut PhotocraftApp) {
    if let Some(workspace) = &app.ui.docking {
        let layout = &workspace.layout;
        if !valid(layout) {
            return;
        }
        let pro = app.ui.theme.is_pro();
        for group in Group::ALL {
            *group.shown_mut(&mut app.ui.panels) = PANELS.iter().filter(|p| p.2 == group).any(|p| layout.contains(&p.0.to_owned()));
            for (index, label) in group.tabs(pro).iter().enumerate() {
                if let Some(id) = normalize(label) {
                    if layout.contains(&id.to_owned()) {
                        app.ui.dock.show_tab(group, index, pro);
                    } else {
                        app.ui.dock.hide_tab(group, index, pro);
                    }
                }
            }
            let previous = *group.tab_mut(&mut app.ui.dock_tabs);
            if !group.tabs(pro).get(previous).and_then(|name| normalize(name)).is_some_and(|id| panel_active(layout, id))
                && let Some(selected) = group.tabs(pro).iter().position(|name| normalize(name).is_some_and(|id| panel_active(layout, id)))
            {
                *group.tab_mut(&mut app.ui.dock_tabs) = selected;
            }
        }
    }
}

/// Is `panel` the selected tab of its group (or an accordion entry)?
fn panel_active(layout: &Layout<String>, panel: &str) -> bool {
    layout
        .root
        .iter()
        .chain(layout.floating.iter().map(|window| &window.root))
        .flat_map(tree_groups)
        .find(|group| group.panels.iter().any(|id| id == panel))
        .is_some_and(|group| group.panels.get(group.active).is_some_and(|id| id == panel))
}
