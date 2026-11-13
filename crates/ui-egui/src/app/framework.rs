//! The craft framework's runtime on PhotoCraft's context, hosted through `craft::ui::shell::embed`
//! (craft-libs docs/craft-framework.md 2.2, "Host the runtime from your own eframe `App`"):
//! installed once with PhotoCraft's fonts, then given PhotoCraft's preferences before each frame.
//!
//! eframe's `ui` would also call `embed::before_ui`, which needs craft-ui-shell's `app` feature;
//! that feature turns on egui's `default_fonts`, which PhotoCraft leaves out (it ships them
//! deflated). Its two jobs are PhotoCraft's own here: `work_area::fit_window` fits the window to
//! its monitor, and the Windows window comes from PhotoCraft's title bar code.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use craft::host::target::{Backend, ShellInfo};
use craft::host::{AppInfo, Host, SectionStore};
use craft::i18n::Direction;
use craft::keymap::Overrides;
use craft::theme::{ModeChoice, ThemeId};
use craft::ui::dialog::DialogHost;
use craft::ui::fonts::UiFonts;
use craft::ui::runtime::commands::{CommandInput, CommandRef, NavCommand};
use craft::ui::runtime::env::{RootEnv, Scale};
use craft::ui::runtime::frame::{ChromeToggle, FrameInput, FrameOutput};
use craft::ui::runtime::{InstallError, InstallSpec};
use craft::ui::shell::embed;
use craft::window::system::SystemWindows;
use photocraft_engine::Session;
use serde_json::Value;

use super::PhotocraftApp;
use crate::i18n;
use crate::theme::ThemeKind;

/// Installs the runtime on `ctx` with `fonts`, unless it is there already (a language change, or
/// `setup_context` again for the same context).
pub(crate) fn install(ctx: &egui::Context, fonts: &UiFonts<'_>) {
    let spec = InstallSpec::new(ShellInfo::default().backend(Backend::from_env()), Arc::new(SystemWindows));
    match embed::install(ctx, spec, fonts) {
        Ok(_) | Err(InstallError::AlreadyInstalled) => {}
        Err(error) => log::error!("the craft runtime could not be installed: {error}"),
    }
}

/// What PhotoCraft keeps for the framework: the `Host` its components read services from, and
/// the host of the framework dialogs it opens.
pub struct Framework {
    sections: Arc<PrefSections>,
    host: Arc<Host>,
    /// The UI language `host` speaks.
    lang: &'static str,
    pub dialogs: DialogHost,
    /// The runtime's keymap overrides. The runtime rebuilds its keymap when the `Arc` changes, so
    /// it is replaced only when the bindings it carries change.
    overrides: Arc<Overrides>,
    /// The preferences revision `overrides` was last checked against.
    overrides_rev: Option<u64>,
    /// The chrome toggles the runtime took from the keys since the last frame, for `logic`.
    pub(crate) chrome_toggles: Vec<ChromeToggle>,
}

impl Default for Framework {
    fn default() -> Self {
        let sections = Arc::new(PrefSections::default());
        let lang = "en";
        Framework {
            host: Arc::new(build_host(&sections, lang)),
            sections,
            lang,
            dialogs: DialogHost::new(),
            overrides: Arc::new(Overrides::new()),
            overrides_rev: None,
            chrome_toggles: Vec::new(),
        }
    }
}

fn build_host(sections: &Arc<PrefSections>, lang: &str) -> Host {
    let i18n = i18n::framework_i18n();
    let mut builder = Host::builder(AppInfo { id: "photocraft", name: "PhotoCraft", version: env!("CARGO_PKG_VERSION") })
        .i18n(Arc::clone(i18n))
        .settings(Arc::clone(sections) as Arc<dyn SectionStore>)
        .jobs(craft::host::default_jobs());
    if let Some(lang) = i18n.from_code(lang) {
        builder = builder.lang(lang);
    }
    builder.build()
}

impl Framework {
    /// The input for the coming frame, from PhotoCraft's preferences.
    fn frame_input(&mut self, session: &mut Session, ctx: &egui::Context, theme: ThemeKind) -> FrameInput {
        self.sections.sync(session);
        let lang = i18n::current().code();
        if lang != self.lang {
            self.lang = lang;
            self.host = Arc::new(build_host(&self.sections, lang));
        }
        let rtl = i18n::framework_i18n().from_code(lang).is_some_and(|l| i18n::framework_i18n().rtl(l));
        let (theme, mode) = theme_of(theme);
        let mut root = RootEnv::new()
            .theme(theme)
            .mode(mode)
            .scale(Scale::new(crate::prefs_ui::interface_zoom(session.prefs().interface.ui_scale, ctx)))
            .direction(if rtl { Direction::Rtl } else { Direction::Ltr });
        // Photoshop's Tab hides the panels, Shift+Tab all but the tools, over the canvas.
        root.tab_toggles_chrome = true;
        FrameInput::new(root).host(Arc::clone(&self.host)).commands(CommandInput::new().overrides(Arc::clone(&self.overrides)))
    }

    /// The runtime's keymap overrides from PhotoCraft's own bindings: the chrome commands it
    /// dispatches for PhotoCraft (`shortcut_dispatch::FRAMEWORK_OWNED`) and the document canvas's
    /// view commands (`shortcut_dispatch::VIEW_OWNED`) get PhotoCraft's keys for them, and the pane moves are unbound, because PhotoCraft's own table binds F6 and Shift+F6
    /// (Window › Color, Select › Modify › Feather) until those are a framework `CommandSet`.
    fn set_overrides(&mut self, rev: u64, chrome: [Vec<String>; 2], view: [Vec<String>; 4]) {
        self.overrides_rev = Some(rev);
        let mut overrides = Overrides::new();
        for command in [NavCommand::NextPane, NavCommand::PreviousPane] {
            overrides.insert(CommandRef::from(command).key(), Vec::new());
        }
        for ((_, command), texts) in crate::shortcut_dispatch::FRAMEWORK_OWNED.iter().zip(chrome) {
            overrides.insert(CommandRef::from(*command).key(), texts);
        }
        for ((_, command), texts) in crate::shortcut_dispatch::VIEW_OWNED.iter().zip(view) {
            overrides.insert(CommandRef::from(*command).key(), texts);
        }
        if *self.overrides != overrides {
            self.overrides = Arc::new(overrides);
        }
    }
}

/// PhotoCraft's theme as the framework's theme and mode. Pro (Medium Gray), PhotoCraft's default
/// dark theme, is the framework's ArtCraft dark, the suite default.
pub(crate) fn theme_of(kind: ThemeKind) -> (ThemeId, ModeChoice) {
    let id = match kind {
        ThemeKind::ProMedium => "craft.artcraft",
        ThemeKind::Pro | ThemeKind::ProLight => "craft.pro",
        ThemeKind::ProLightGray => "craft.pro-light-gray",
        ThemeKind::Studio | ThemeKind::StudioLight => "craft.studio",
        ThemeKind::Classic => "craft.classic",
        ThemeKind::SolarizedDark => "craft.solarized-dark",
        ThemeKind::Adwaita | ThemeKind::AdwaitaDark => "craft.adwaita",
        ThemeKind::BreezeLight | ThemeKind::BreezeDark => "craft.breeze",
        ThemeKind::SoftLight | ThemeKind::SoftDark => "craft.soft",
    };
    let theme = ThemeId::parse(id).unwrap_or_else(|_| ThemeId::suite_default());
    (theme, if kind.is_dark() { ModeChoice::Dark } else { ModeChoice::Light })
}

impl PhotocraftApp {
    /// Before each frame (`raw_input_hook`): the runtime's output of the last frame, and its
    /// input for this one. Of the output, PhotoCraft runs the chrome toggles (in `logic`).
    pub(crate) fn framework_before_input(&mut self, ctx: &egui::Context) {
        // The bindings come from walking the whole command table: only after a preferences change.
        let rev = self.session.prefs.rev();
        if self.framework.overrides_rev != Some(rev) {
            let chrome = crate::shortcut_dispatch::owned_texts(self, &crate::shortcut_dispatch::FRAMEWORK_OWNED);
            let view = crate::shortcut_dispatch::owned_texts(self, &crate::shortcut_dispatch::VIEW_OWNED);
            self.framework.set_overrides(rev, chrome, view);
        }
        embed::before_input(ctx, |output: FrameOutput| {
            self.framework.chrome_toggles.extend(output.chrome_toggles.into_iter().map(|(_, toggle)| toggle));
            self.framework.frame_input(&mut self.session, ctx, self.ui.theme)
        });
    }

    /// After PhotoCraft's own dialogs (`ui_body`): the framework dialogs it keeps open.
    pub(crate) fn framework_dialogs(&mut self, ui: &mut egui::Ui) {
        self.framework.dialogs.run(ui);
    }
}

/// The framework's settings sections, kept in PhotoCraft's preferences (`Preferences::sections`).
/// Components write from inside a pass, where the session isn't reachable: the tables wait here
/// and reach the preferences before the next frame.
#[derive(Default)]
struct PrefSections(Mutex<Tables>);

#[derive(Default)]
struct Tables {
    tables: BTreeMap<String, toml::Table>,
    /// A component wrote since the last sync.
    written: bool,
    /// The preferences' sections as last synced, to notice a change from elsewhere (a reset,
    /// the control channel).
    seen: BTreeMap<String, Value>,
}

impl PrefSections {
    /// Writes the components' changes into the preferences, or takes a change made there.
    fn sync(&self, session: &mut Session) {
        let mut t = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if std::mem::take(&mut t.written) {
            let json: BTreeMap<String, Value> = t.tables.iter().filter_map(|(key, table)| Some((key.clone(), serde_json::to_value(table).ok()?))).collect();
            if session.prefs().sections != json {
                session.edit_prefs(|p| p.sections = json.clone());
            }
            t.seen = json;
        } else if session.prefs().sections != t.seen {
            let seen = session.prefs().sections.clone();
            t.tables = seen.iter().filter_map(|(key, value)| Some((key.clone(), serde_json::from_value(value.clone()).ok()?))).collect();
            t.seen = seen;
        }
    }
}

impl SectionStore for PrefSections {
    fn read(&self, key: &str) -> Option<toml::Table> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).tables.get(key).cloned()
    }

    fn write(&self, key: &str, table: toml::Table) {
        let mut t = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        t.tables.insert(key.to_owned(), table);
        t.written = true;
    }
}

/// One frame with the runtime installed on a fresh context, running only the root window's close
/// path with PhotoCraft's decision, as `logic` does.
#[cfg(test)]
pub(crate) fn close_path_frame(app: &mut PhotocraftApp, input: egui::RawInput) -> egui::FullOutput {
    let ctx = egui::Context::default();
    install(&ctx, &UiFonts::new(&craft::ui::fonts::I18nScripts));
    ctx.run_ui(input, |ui| embed::logic(ui.ctx(), || crate::discard_ui::close_decision(app)))
}

#[cfg(test)]
mod tests {
    /// Every PhotoCraft theme names a framework built-in, in its own mode; Pro (Medium Gray) is
    /// ArtCraft dark.
    #[test]
    fn every_theme_maps_to_a_builtin_in_its_mode() {
        let builtins = craft::theme::builtins();
        for kind in crate::theme::ThemeKind::ALL {
            let (id, mode) = super::theme_of(kind);
            assert!(builtins.iter().any(|t| t.id == id), "{kind:?}: {} is no built-in theme", id.as_str());
            assert_eq!(mode, if kind.is_dark() { craft::theme::ModeChoice::Dark } else { craft::theme::ModeChoice::Light }, "{kind:?}");
        }
        assert_eq!(super::theme_of(crate::theme::ThemeKind::ProMedium).0.as_str(), "craft.artcraft");
    }
}
