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
use craft::theme::{AppDefaults, AppSizes, ModeChoice, Size, SuiteChoice, ThemeId, ThemePair, resolve_sized};
use craft::theme_store::{SelectionChange, SuiteClient};
use craft::ui::dialog::{DialogHost, DialogId, dialog_id};
use craft::ui::fonts::UiFonts;
use craft::ui::runtime::commands::{CommandInput, CommandRef, NavCommand};
use craft::ui::runtime::env::{RootEnv, Scale};
use craft::ui::runtime::frame::{ChromeToggle, FrameInput, FrameOutput};
use craft::ui::runtime::{InstallError, InstallSpec};
use craft::ui::shell::embed;
use craft::window::system::SystemWindows;
use photocraft_engine::Session;
use photocraft_engine::prefs::{AppearanceMode, Preferences};
use serde_json::{Map, Value};

use super::PhotocraftApp;
use crate::i18n;
use crate::state::{Dialog, DialogKind, UiState};
use crate::theme::AppTheme;

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
    /// The suite's theme selection, shared with the other Craft apps.
    suite: SuiteClient,
    /// The theme slots and mode as PhotoCraft's preferences and the suite last agreed on them.
    synced: Option<ThemePrefs>,
    /// The preferences' rev `synced` was taken at: with no suite move since, nothing to sync.
    synced_rev: Option<u64>,
    /// Whether the start's agreement ran: the suite's chosen fields over the preferences, once the
    /// suite's selection is read.
    started: bool,
    /// How many times `theme_for` ran, for tests that count resolves.
    #[cfg(test)]
    pub(crate) theme_resolves: std::sync::atomic::AtomicUsize,
}

/// Why a theme can't show now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ThemeUnavailable {
    /// A library theme picked for the suite that the suite hasn't loaded yet.
    Loading,
    /// A library theme in the suite's library that nothing picked, so the suite doesn't load it.
    NotPicked,
    /// No theme the suite knows.
    Unknown,
}

impl ThemeUnavailable {
    /// The message for theme `id`: a refusal for an unknown one, a note for one still loading.
    pub(crate) fn message(self, id: &ThemeId) -> String {
        match self {
            ThemeUnavailable::Loading | ThemeUnavailable::NotPicked => format!("theme `{id}` is still loading"),
            ThemeUnavailable::Unknown => format!("unknown theme `{id}`"),
        }
    }
}

/// The preferences that choose the theme: the appearance mode and the theme id of each slot.
#[derive(Clone, Debug, PartialEq)]
struct ThemePrefs {
    mode: AppearanceMode,
    dark: String,
    light: String,
}

impl ThemePrefs {
    fn of(prefs: &Preferences) -> Self {
        let i = &prefs.interface;
        ThemePrefs { mode: i.appearance_mode, dark: i.dark_theme.clone(), light: i.light_theme.clone() }
    }

    /// What the suite shows: its chosen fields, else PhotoCraft's defaults.
    fn of_suite(choice: &SuiteChoice) -> Self {
        let defaults = defaults();
        let shown = choice.with_defaults(&defaults);
        ThemePrefs { mode: app_mode(shown.mode), dark: shown.theme.dark.as_str().to_owned(), light: shown.theme.light.as_str().to_owned() }
    }

    /// The suite fields that differ between `self` and `before`.
    fn change_from(&self, before: &ThemePrefs) -> SelectionChange {
        let id = |value: &str| ThemeId::parse(value).ok();
        SelectionChange {
            theme_dark: (self.dark != before.dark).then(|| id(&self.dark)).flatten(),
            theme_light: (self.light != before.light).then(|| id(&self.light)).flatten(),
            mode: (self.mode != before.mode).then(|| mode_choice(self.mode)),
            ..SelectionChange::default()
        }
    }
}

fn app_mode(mode: ModeChoice) -> AppearanceMode {
    match mode {
        ModeChoice::System => AppearanceMode::Auto,
        ModeChoice::Dark => AppearanceMode::Dark,
        ModeChoice::Light => AppearanceMode::Light,
    }
}

fn mode_choice(mode: AppearanceMode) -> ModeChoice {
    match mode {
        AppearanceMode::Auto => ModeChoice::System,
        AppearanceMode::Dark => ModeChoice::Dark,
        AppearanceMode::Light => ModeChoice::Light,
    }
}

/// PhotoCraft's theme in the dark mode when nobody chose one: Pro (Medium Gray).
const DEFAULT_DARK: &str = "craft.artcraft";
/// PhotoCraft's theme in the light mode when nobody chose one: Pro (Light).
const DEFAULT_LIGHT: &str = "craft.pro";

/// What PhotoCraft shows for each suite field nobody has chosen: Pro (Medium Gray) in the dark
/// mode, Pro in the light, and the dark mode.
fn defaults() -> AppDefaults {
    let theme = |id: &str| ThemeId::parse(id).unwrap_or_else(|_| ThemeId::suite_default());
    AppDefaults {
        theme: ThemePair { light: theme(DEFAULT_LIGHT), dark: theme(DEFAULT_DARK) },
        mode: ModeChoice::Dark,
        size: Size::default(),
        sizes: AppSizes::default(),
    }
}

/// The root values PhotoCraft gives the runtime before the theme, mode, scale and direction: the
/// contrast, size and sizes a theme's tokens resolve under.
fn base_root() -> RootEnv {
    let mut root = RootEnv::new();
    // Photoshop's Tab hides the panels, Shift+Tab all but the tools, over the canvas.
    root.tab_toggles_chrome = true;
    root
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
            suite: SuiteClient::memory(),
            synced: None,
            synced_rev: None,
            started: false,
            #[cfg(test)]
            theme_resolves: Default::default(),
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
    fn frame_input(&mut self, session: &mut Session, ctx: &egui::Context, theme: &AppTheme) -> FrameInput {
        self.sections.sync(session);
        let lang = i18n::current().code();
        if lang != self.lang {
            self.lang = lang;
            self.host = Arc::new(build_host(&self.sections, lang));
        }
        let rtl = i18n::framework_i18n().from_code(lang).is_some_and(|l| i18n::framework_i18n().rtl(l));
        let root = match theme.library() {
            Some(def) => base_root().library_theme(Arc::clone(def)),
            None => base_root().theme(theme.id().clone()),
        };
        let root = root
            .mode(if theme.is_dark() { ModeChoice::Dark } else { ModeChoice::Light })
            .scale(Scale::new(crate::prefs_ui::interface_zoom(session.prefs().interface.ui_scale, ctx)))
            .direction(if rtl { Direction::Rtl } else { Direction::Ltr });
        FrameInput::new(root).host(Arc::clone(&self.host)).commands(CommandInput::new().overrides(Arc::clone(&self.overrides)))
    }

    /// Opens the suite folder this machine shares with the other Craft apps, in place of the
    /// in-memory suite the app starts with.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn open_suite(&mut self, ctx: &egui::Context) {
        use craft::theme_store::{StoreOptions, SuiteDir, SuiteWatch};
        let ctx = ctx.clone();
        let options = StoreOptions::new(concat!("photocraft ", env!("CARGO_PKG_VERSION")));
        self.suite = SuiteClient::new(SuiteWatch::open(&SuiteDir::resolve(false), options, move || ctx.request_repaint()));
        self.synced = None;
        self.synced_rev = None;
        self.started = false;
    }

    /// Brings the preferences and the suite's selection together: a change made in the
    /// preferences goes to the suite, a change another app made to the suite goes to the
    /// preferences. At the start, once the suite's selection is read, the suite's chosen fields
    /// win and the preferences' own choices fill the fields it has not chosen; until then the
    /// preferences show. Runs only when the suite moved or the preferences changed. Returns
    /// whether the theme on screen may need to change.
    pub(crate) fn sync_theme(&mut self, session: &mut Session) -> bool {
        let moved = self.suite.sync();
        if !moved && self.synced_rev == Some(session.prefs.rev()) {
            return false;
        }
        let mine = ThemePrefs::of(session.prefs());
        let suite = moved.then(|| ThemePrefs::of_suite(&self.suite.shown()));
        let previous = self.synced.take();
        let agreed = match &previous {
            _ if !self.suite.selection_read() => mine.clone(),
            _ if !self.started => {
                self.started = true;
                let chosen = self.suite.accepted().suite.clone();
                let blank = ThemePrefs::of_suite(&SuiteChoice::default());
                let mut start = mine.clone();
                let mut change = SelectionChange::default();
                match &chosen.theme_dark {
                    Some(id) => start.dark = id.as_str().to_owned(),
                    None if mine.dark != blank.dark => change.theme_dark = ThemeId::parse(&mine.dark).ok(),
                    None => {}
                }
                match &chosen.theme_light {
                    Some(id) => start.light = id.as_str().to_owned(),
                    None if mine.light != blank.light => change.theme_light = ThemeId::parse(&mine.light).ok(),
                    None => {}
                }
                match chosen.mode {
                    Some(mode) => start.mode = app_mode(mode),
                    None if mine.mode != blank.mode => change.mode = Some(mode_choice(mine.mode)),
                    None => {}
                }
                self.suite.change(change);
                start
            }
            Some(agreed) if mine != *agreed => {
                self.suite.change(mine.change_from(agreed));
                mine.clone()
            }
            Some(agreed) => match suite {
                Some(suite) if suite != *agreed => suite,
                _ => agreed.clone(),
            },
            None => mine.clone(),
        };
        if agreed != mine {
            session.edit_prefs(|p| {
                p.interface.appearance_mode = agreed.mode;
                p.interface.dark_theme = agreed.dark.clone();
                p.interface.light_theme = agreed.light.clone();
            });
        }
        let changed = moved || previous.as_ref() != Some(&agreed);
        self.synced = Some(agreed);
        self.synced_rev = Some(session.prefs.rev());
        changed
    }

    /// The theme `id` to show in the dark or light mode: PhotoCraft's palette for it, else its
    /// tokens resolved from the suite's definition under the root's contrast, size and sizes, as
    /// the runtime resolves its own. A theme the suite can't resolve shows as PhotoCraft's default
    /// for the mode.
    pub(crate) fn theme_for(&self, id: &ThemeId, dark: bool, ctx: &egui::Context) -> AppTheme {
        #[cfg(test)]
        self.theme_resolves.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(theme) = AppTheme::palette(id, dark) {
            return theme;
        }
        let Some(def) = self.suite.theme(id) else {
            let defaults = defaults().theme;
            return AppTheme::palette(if dark { &defaults.dark } else { &defaults.light }, dark).unwrap_or_default();
        };
        let root = base_root().mode(if dark { ModeChoice::Dark } else { ModeChoice::Light });
        let resolved = resolve_sized(&def, root.appearance(ctx), None, &root.sizes);
        AppTheme::resolved(Arc::new(def), dark, &resolved)
    }

    /// The suite, for tests that put themes in its library.
    #[cfg(test)]
    pub(crate) fn suite(&self) -> &SuiteClient {
        &self.suite
    }

    /// Whether `shown` is what [`theme_for`](Self::theme_for) gives for the slot id `slot` in
    /// `shown`'s mode because the suite can't resolve it: the mode's default palette, and `slot`
    /// a theme the suite doesn't know. Checked without resolving or copying a theme.
    pub(crate) fn shows_fallback_for(&self, slot: &str, shown: &AppTheme) -> bool {
        let default = if shown.is_dark() { DEFAULT_DARK } else { DEFAULT_LIGHT };
        shown.id().as_str() == default
            && shown.library().is_none()
            && ThemeId::parse(slot).is_ok_and(|id| self.theme_availability(&id) == Err(ThemeUnavailable::Unknown))
    }

    /// Whether theme `id` can show now: the suite has it as a built-in or a loaded library theme.
    pub(crate) fn theme_availability(&self, id: &ThemeId) -> Result<(), ThemeUnavailable> {
        if self.suite.has_theme(id) {
            Ok(())
        } else if self.suite.is_loading(id) {
            Err(ThemeUnavailable::Loading)
        } else if self.suite.in_library(id) {
            Err(ThemeUnavailable::NotPicked)
        } else {
            Err(ThemeUnavailable::Unknown)
        }
    }

    /// The name of theme `id` for a menu: PhotoCraft's own name for its palettes, else the suite's.
    pub(crate) fn theme_name(&self, id: &ThemeId, dark: bool) -> String {
        AppTheme::palette_label(id.as_str(), dark)
            .map(str::to_owned)
            .or_else(|| self.suite.theme(id).map(|def| def.name))
            .unwrap_or_else(|| id.as_str().to_owned())
    }

    /// The runtime's keymap overrides from PhotoCraft's own bindings: the chrome commands it
    /// dispatches for PhotoCraft (`shortcut_dispatch::FRAMEWORK_OWNED`) get PhotoCraft's keys for
    /// them, and the pane moves are unbound, because PhotoCraft's own table binds F6 and Shift+F6
    /// (Window › Color, Select › Modify › Feather) until those are a framework `CommandSet`.
    fn set_overrides(&mut self, rev: u64, chrome: [Vec<String>; 2]) {
        self.overrides_rev = Some(rev);
        let mut overrides = Overrides::new();
        for command in [NavCommand::NextPane, NavCommand::PreviousPane] {
            overrides.insert(CommandRef::from(command).key(), Vec::new());
        }
        for ((_, command), texts) in crate::shortcut_dispatch::FRAMEWORK_OWNED.iter().zip(chrome) {
            overrides.insert(CommandRef::from(*command).key(), texts);
        }
        if *self.overrides != overrides {
            self.overrides = Arc::new(overrides);
        }
    }
}

impl PhotocraftApp {
    /// Before each frame (`raw_input_hook`): the runtime's output of the last frame, and its
    /// input for this one. Of the output, PhotoCraft runs the chrome toggles (in `logic`).
    pub(crate) fn framework_before_input(&mut self, ctx: &egui::Context) {
        // The bindings come from walking the whole command table: only after a preferences change.
        let rev = self.session.prefs.rev();
        if self.framework.overrides_rev != Some(rev) {
            let chrome = crate::shortcut_dispatch::framework_owned_texts(self);
            self.framework.set_overrides(rev, chrome);
        }
        if self.framework.sync_theme(&mut self.session) {
            crate::prefs_ui::show_selected_theme(self, ctx);
        }
        embed::before_input(ctx, |output: FrameOutput| {
            self.framework.chrome_toggles.extend(output.chrome_toggles.into_iter().map(|(_, toggle)| toggle));
            self.framework.frame_input(&mut self.session, ctx, &self.ui.theme)
        });
    }

    /// After PhotoCraft's own dialogs (`ui_body`): the framework dialogs it keeps open.
    pub(crate) fn framework_dialogs(&mut self, ui: &mut egui::Ui) {
        self.framework.dialogs.run(ui);
    }

    /// Opens PhotoCraft's own dialog `kind` on top of the one dialog stack, over the dialog its fields name as its
    /// parent. Returns its id.
    pub fn open_dialog(&mut self, kind: DialogKind, fields: Map<String, Value>) -> u64 {
        open_dialog(&mut self.ui, &mut self.framework.dialogs, kind, fields)
    }

    /// Closes PhotoCraft's own dialog `id`: it leaves the stack with the framework dialogs opened over it.
    pub fn close_dialog(&mut self, id: u64) -> Option<Dialog> {
        let at = self.ui.dialogs.iter().position(|d| d.id == id)?;
        self.framework.dialogs.close_beside(stack_id(id));
        let closed = self.ui.dialogs.remove(at);
        // Closing a dialog also closes the ones opened over it: the two lists never part.
        let host = &self.framework.dialogs;
        self.ui.dialogs.retain(|d| host.stack().ids().any(|s| s == stack_id(d.id)));
        Some(closed)
    }

    /// Closes PhotoCraft's own dialogs for which `keep` is false, each as [`close_dialog`](Self::close_dialog) does.
    pub fn retain_dialogs(&mut self, mut keep: impl FnMut(&Dialog) -> bool) {
        let closed: Vec<u64> = self.ui.dialogs.iter().filter(|d| !keep(d)).map(|d| d.id).collect();
        for id in closed {
            self.close_dialog(id);
        }
    }

    /// Whether a dialog is open, PhotoCraft's own or a framework one: the canvas, shortcuts, menus and tools under it
    /// take no input.
    pub fn dialog_open(&self) -> bool {
        !self.framework.dialogs.stack().is_empty()
    }

    /// How many dialogs are open, PhotoCraft's own and framework ones.
    pub fn dialog_count(&self) -> usize {
        self.framework.dialogs.stack().ids().count()
    }
}

/// [`PhotocraftApp::open_dialog`] for code that holds other parts of the app at the same time.
pub(crate) fn open_dialog(ui: &mut UiState, host: &mut DialogHost, kind: DialogKind, fields: Map<String, Value>) -> u64 {
    let parent = crate::dialogs::parent_of(&fields).map(stack_id);
    let id = ui.alloc_id();
    ui.dialogs.push(Dialog { id, kind, fields });
    host.open_beside(stack_id(id), parent);
    id
}

/// The framework stack's id of PhotoCraft's own dialog `id`: control requests address both kinds
/// by one `dialog` id, PhotoCraft's `u64` for its own dialogs and the stack id for hosted ones.
pub(crate) fn stack_id(id: u64) -> DialogId {
    dialog_id(("photocraft.dialog", id))
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
    /// Every theme PhotoCraft has a palette for names a framework built-in.
    #[test]
    fn every_palette_theme_is_a_builtin() {
        let builtins = craft::theme::builtins();
        for theme in crate::theme::AppTheme::all() {
            assert!(builtins.iter().any(|t| t.id == *theme.id()), "{} is no built-in theme", theme.id().as_str());
        }
        assert_eq!(crate::theme::AppTheme::pro_medium().id().as_str(), "craft.artcraft");
    }
}
