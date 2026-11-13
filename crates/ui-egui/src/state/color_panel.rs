use super::{Deserialize, Serialize};

/// Selected tab per dock card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockTabs {
    pub properties: usize,
    pub color: usize,
    pub layers: usize,
    /// Navigator | Histogram | Info.
    pub navigator: usize,
    /// History | Actions.
    pub history: usize,
    /// Character | Paragraph.
    pub character: usize,
    /// Brushes presets.
    pub brushes: usize,
}

/// Color panel state: the framework panel's settings, and the Color Picker's Only Web Colors,
/// which shares their saved preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ColorPanelState {
    /// What the panel shows, its ramp and which colour it edits (#6000).
    #[serde(flatten)]
    pub panel: craft::color::ui::ColorPanelState,
    /// Color Picker › Only Web Colors, kept across opens like Photoshop (#4995). `web_only` is
    /// its name before the camelCase rename (#6000).
    #[serde(default, alias = "web_only")]
    pub web_only: bool,
}
