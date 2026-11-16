//! The panel rail's commands: one toggle per rail icon, lit while its panel's group shows, and the rail that shows
//! them. A fired command applies the framework's rail actions to the workspace.

use craft::ui::contract::CommandSet;
use craft::ui::docking::{rail_actions, rail_lit};
use craft::ui::rails::{PanelRail, RailTool};
use craft::ui::runtime::commands::{CommandRef, CommandSet as _, CommandState};

use super::PhotocraftApp;

/// The rail's icons, each the toggle of one panel group. Application scope: a key or a script reaches them from
/// anywhere in the window. Ids read `panelRail.<icon>`, and each also answers to `window.rail.<icon>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, CommandSet)]
#[command(prefix = "panelRail")]
pub(crate) enum RailCommand {
    /// Properties and Adjustments.
    #[command(label = "Properties", icon = "adjustments", kind = "toggle", alias = "window.rail.properties")]
    Properties,
    /// Navigator, Histogram and Info.
    #[command(label = "Navigator", icon = "navigation", kind = "toggle", alias = "window.rail.navigator")]
    Navigator,
    /// Color, Swatches, Gradients and Patterns.
    #[command(label = "Color & Swatches", icon = "palette", kind = "toggle", alias = "window.rail.colorSwatches")]
    ColorSwatches,
    /// Layers, Channels and Paths.
    #[command(label = "Layers", icon = "layers", kind = "toggle", alias = "window.rail.layers")]
    Layers,
    /// History, Actions and Layer Comps.
    #[command(label = "History", icon = "clock", kind = "toggle", alias = "window.rail.history")]
    History,
    /// The Brushes presets.
    #[command(label = "Brushes", icon = "brush", kind = "toggle", alias = "window.rail.brushes")]
    Brushes,
}

impl RailCommand {
    /// Every rail command, top to bottom.
    const ALL: [RailCommand; 6] =
        [RailCommand::Properties, RailCommand::Navigator, RailCommand::ColorSwatches, RailCommand::Layers, RailCommand::History, RailCommand::Brushes];

    /// The panel whose group the icon shows and hides.
    fn panel(self) -> &'static str {
        match self {
            RailCommand::Properties => "properties",
            RailCommand::Navigator => "navigator",
            RailCommand::ColorSwatches => "color",
            RailCommand::Layers => "layers",
            RailCommand::History => "history",
            RailCommand::Brushes => "brushes",
        }
    }
}

/// Each rail command checked while its panel's group is in the workspace and not minimized.
pub(crate) fn states(app: &PhotocraftApp) -> Vec<(CommandRef, CommandState)> {
    let layout = &app.ui.docking.layout;
    RailCommand::ALL.into_iter().map(|command| (command.into(), CommandState::new().checked(rail_lit(layout, &command.panel().to_owned())))).collect()
}

/// Runs a fired rail command: minimizes a shown group, restores a minimized one, closes a floating one's window and
/// opens a hidden one. `false` when the command is not a rail command.
pub(crate) fn run_fired(app: &mut PhotocraftApp, _: &egui::Context, command: &CommandRef) -> bool {
    let Some(command) = RailCommand::from_ref(command) else { return false };
    let actions = rail_actions(&app.ui.docking.layout, &command.panel().to_owned());
    if let Err(error) = super::panel_docking::apply(app, actions) {
        app.ui.status = error;
    }
    true
}

/// Shows the panel rail. Its icons fire the rail commands, which [`run_fired`] runs with the app's other commands, so
/// the rail reads no app state of its own: each icon's lit look is its command's checked state. The canvas panel's
/// document area shows it at its end edge while the rail is on.
pub(crate) fn rail(ui: &mut egui::Ui) {
    let tools: Vec<RailTool<&'static str>> = RailCommand::ALL.into_iter().map(|command| RailTool::new(command.panel(), command)).collect();
    PanelRail::new(tl!("Panels"), &tools).show_rail(ui, |_| {});
}
