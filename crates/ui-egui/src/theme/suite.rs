//! The Craft suite's theme in PhotoCraft: the suite folder's watcher, PhotoCraft's choice
//! (`interface.suiteAppearance`) and the token sets for egui's two style slots.
//!
//! `craft.app-default` stands for PhotoCraft's own themes: the Dark slot gets `darkTheme`, the
//! Light slot `lightTheme`, exactly as before suite themes. Any other theme goes through
//! [`Tokens::from_resolved`].

use super::{ThemeKind, Tokens};
use craft_theme::{AppChoice, AppDefaults, Mode, Resolution, Selection, SuiteTheme, ThemeId, Variant};
use craft_theme_store::{Accepted, Problem, SelectionChange, StoreError, StoreOptions, SuiteChoice, SuiteClient, SuiteWatch};

/// What PhotoCraft shows while it follows the suite and no suite theme has been picked.
pub fn app_defaults() -> AppDefaults {
    AppDefaults { theme: ThemeId::suite_default(), mode: Mode::Dark }
}

/// The variant for a mode that follows the system while the OS gives no answer.
pub const FALLBACK: Variant = Variant::Dark;

/// Store options for this app: who writes, and the selection PhotoCraft saw last.
pub fn store_options(last: Accepted) -> StoreOptions {
    StoreOptions::new(concat!("photocraft ", env!("CARGO_PKG_VERSION"))).last_accepted(last)
}

/// Opens the suite watcher once the saved preferences are read (the shell's choice of folder and
/// wake-up; see [`crate::Services::suite_watch`]).
pub type OpenWatchFn = Box<dyn FnOnce(StoreOptions) -> SuiteWatch>;

#[derive(Clone, Debug, PartialEq)]
struct Key {
    generation: u64,
    selection: Selection,
    own: (ThemeKind, ThemeKind),
}

/// PhotoCraft's one seam to the suite: changes go out through [`SuiteClient`], which keeps the
/// accepted state, the change in flight, conflicts and failed writes; this adds what PhotoCraft
/// derives from it (the effective selection and the token sets for egui's slots).
pub struct Suite {
    client: SuiteClient,
    cache: Option<(Key, (Tokens, Tokens))>,
}

impl Default for Suite {
    fn default() -> Self {
        Suite::new(SuiteWatch::memory())
    }
}

impl Suite {
    pub fn new(watch: SuiteWatch) -> Self {
        Suite { client: SuiteClient::new(watch), cache: None }
    }

    /// Opens the watcher with `open`, or in memory without one (tests, agents, the web build).
    pub fn open(open: Option<OpenWatchFn>, last: Accepted) -> Self {
        let options = store_options(last);
        Suite::new(match open {
            Some(open) => open(options),
            None => SuiteWatch::open(&craft_theme_store::SuiteDir::memory(), options, || {}),
        })
    }

    pub fn client(&self) -> &SuiteClient {
        &self.client
    }

    /// For import, delete, export and rescan, which no frame waits on.
    pub fn watch(&self) -> &SuiteWatch {
        self.client.watch()
    }

    /// Once per frame: one atomic load unless the watcher has a new snapshot. Returns whether
    /// anything shown may have changed.
    pub fn sync(&mut self) -> bool {
        self.client.sync()
    }

    /// The suite's selection as shown: accepted, with the changes on their way.
    pub fn shown(&self) -> SuiteChoice {
        self.client.shown()
    }

    /// The state the suite last confirmed (kept in the preferences as `lastSuite`).
    pub fn accepted(&self) -> &Accepted {
        self.client.accepted()
    }

    /// The selection that applies to PhotoCraft for `choice`.
    pub fn effective(&self, choice: &AppChoice) -> Selection {
        let defaults = app_defaults();
        choice.effective(Some(&self.shown().with_defaults(&defaults)), &defaults)
    }

    /// The user changed the suite's theme or mode.
    pub fn change(&mut self, change: SelectionChange) {
        self.client.change(change);
    }

    /// Picks `id` as the suite theme.
    pub fn set_theme(&mut self, id: ThemeId) {
        self.change(SelectionChange::theme(id));
    }

    /// Another app's choice blocked the last change: the suite's state and the change.
    pub fn conflict(&self) -> Option<&(Accepted, SelectionChange)> {
        self.client.conflict()
    }

    /// The last change that failed, with the reason.
    pub fn failure(&self) -> Option<&(SelectionChange, StoreError)> {
        self.client.failure()
    }

    pub fn apply_mine(&mut self) {
        self.client.apply_mine();
    }

    pub fn retry(&mut self) {
        self.client.retry();
    }

    pub fn dismiss(&mut self) {
        self.client.dismiss();
    }

    /// Files in the suite folder PhotoCraft can't use; the last accepted state stays.
    pub fn problems(&self) -> &[Problem] {
        self.client.problems()
    }

    /// A theme PhotoCraft can show by this id: its own themes, a built-in or a library theme.
    pub fn knows(&self, id: &ThemeId) -> bool {
        id.is_app_default() || craft_theme::builtin(id).is_some() || self.client.snapshot().index.iter().any(|e| e.id == *id)
    }

    /// The token sets for egui's Light and Dark slots under `choice`, with `own` as PhotoCraft's
    /// own (Light, Dark) themes. Rebuilt only when the snapshot, the effective selection or
    /// `own` changed. While PhotoCraft's own library theme loads, the current look stays.
    pub fn slots(&mut self, choice: &AppChoice, own: (ThemeKind, ThemeKind)) -> (Tokens, Tokens) {
        let own_theme = match choice {
            AppChoice::Own(sel) => Some(sel.theme.clone()),
            AppChoice::FollowSuite => None,
        };
        self.client.set_app_theme(own_theme.clone());
        self.client.sync();
        if let (Some(id), Some((_, slots))) = (&own_theme, &self.cache)
            && self.client.is_loading(id)
        {
            return *slots;
        }
        let key = Key { generation: self.client.snapshot().generation, selection: self.effective(choice), own };
        if let Some((cached, slots)) = &self.cache
            && *cached == key
        {
            return *slots;
        }
        let client = &self.client;
        let slots = resolve_slots(&key.selection, &|id| client.theme(id), own);
        self.cache = Some((key, slots));
        slots
    }
}

/// Each slot resolved with its own variant as the OS answer; a fixed mode gives both the same.
pub fn resolve_slots(selection: &Selection, lookup: &dyn Fn(&ThemeId) -> Option<SuiteTheme>, (light, dark): (ThemeKind, ThemeKind)) -> (Tokens, Tokens) {
    let slot = |system| match craft_theme::resolve(selection, Some(system), FALLBACK, lookup) {
        Resolution::AppDefault { variant: Variant::Light, .. } => Tokens::for_kind(light),
        Resolution::AppDefault { variant: Variant::Dark, .. } => Tokens::for_kind(dark),
        Resolution::Theme(r) => Tokens::from_resolved(&r),
    };
    (slot(Variant::Light), slot(Variant::Dark))
}
