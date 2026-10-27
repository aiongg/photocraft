//! The Craft suite's theme in PhotoCraft: the suite folder's watcher, PhotoCraft's choice
//! (`interface.suiteAppearance`) and the token sets for egui's two style slots.
//!
//! `craft.app-default` stands for PhotoCraft's own themes: the Dark slot gets `darkTheme`, the
//! Light slot `lightTheme`, exactly as before suite themes. Any other theme goes through
//! [`Tokens::from_resolved`].

use super::{ThemeKind, Tokens};
use craft_theme::contrast::Diagnostic;
use craft_theme::{AppChoice, AppDefaults, Mode, Resolution, Selection, SuiteTheme, ThemeId, Variant};
use craft_theme_store::{
    Accepted, FileStamp, Op, OpOutput, Problem, SelectionChange, StoreError, StoreOptions, SuiteChoice, SuiteClient, SuiteWatch, ThemeRecord,
};
use std::collections::HashMap;

/// What PhotoCraft shows while it follows the suite and no suite theme has been picked.
pub fn app_defaults() -> AppDefaults {
    AppDefaults { theme: ThemeId::suite_default(), mode: Mode::Dark }
}

/// The variant for a mode that follows the system while the OS gives no answer.
pub const FALLBACK: Variant = Variant::Dark;

/// Store options for this app: who writes, the selection PhotoCraft saw last and the library
/// theme of its own choice (`None` for its own themes or a built-in). With both, the watcher's
/// first snapshot already holds the themes in use, so the first frame shows them.
pub fn store_options(last: Accepted, app_theme: Option<ThemeId>) -> StoreOptions {
    StoreOptions::new(concat!("photocraft ", env!("CARGO_PKG_VERSION"))).last_accepted(last).app_theme(app_theme.filter(|id| !id.is_builtin()))
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
    shared: bool,
    /// Library themes read for a theme picker's previews, by id.
    previews: HashMap<ThemeId, Preview>,
}

/// A library theme's file, read for its preview: asked for (the request id), or read (`None` when
/// it didn't parse). Read again when the library lists a new stamp.
enum Preview {
    Requested(u64, FileStamp),
    Read(FileStamp, Option<Box<ThemeRecord>>),
}

impl Default for Suite {
    fn default() -> Self {
        Suite::new(SuiteWatch::memory())
    }
}

impl Suite {
    /// A suite only this app sees (in memory): there is nothing to follow, so the per-app
    /// choice is the only one.
    pub fn new(watch: SuiteWatch) -> Self {
        Suite { client: SuiteClient::new(watch), cache: None, shared: false, previews: HashMap::new() }
    }

    /// The suite folder the other Craft apps use, opened by the shell.
    pub fn shared(watch: SuiteWatch) -> Self {
        Suite { shared: true, ..Suite::new(watch) }
    }

    /// Opens the watcher with `open` (the shared folder), or in memory without one (tests,
    /// agents, portable installs, the web build).
    pub fn open(open: Option<OpenWatchFn>, last: Accepted, app_theme: Option<ThemeId>) -> Self {
        let options = store_options(last, app_theme);
        match open {
            Some(open) => Suite::shared(open(options)),
            None => Suite::new(SuiteWatch::open(&craft_theme_store::SuiteDir::memory(), options, || {})),
        }
    }

    /// Whether the suite selection is shared with the other Craft apps (the desktop shell with
    /// the per-user settings folder). Otherwise it is in memory and "Follow suite" is hidden.
    pub fn is_shared(&self) -> bool {
        self.shared
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

    /// The suite themes a theme picker lists: the built-ins in their order, then the library
    /// themes by name.
    pub fn themes(&self) -> Vec<(ThemeId, String)> {
        let mut library: Vec<(ThemeId, String)> = self.client.snapshot().index.iter().map(|e| (e.id.clone(), e.name.clone())).collect();
        library.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()).then_with(|| a.0.cmp(&b.0)));
        craft_theme::builtins().iter().map(|t| (t.id.clone(), t.name.clone())).chain(library).collect()
    }

    /// A theme's name as a picker shows it; the id for one PhotoCraft doesn't know.
    pub fn name(&self, id: &ThemeId) -> String {
        craft_theme::builtin(id)
            .map(|t| t.name.clone())
            .or_else(|| self.client.snapshot().index.iter().find(|e| e.id == *id).map(|e| e.name.clone()))
            .unwrap_or_else(|| id.as_str().to_owned())
    }

    /// True while `id` is a library theme just picked that the watcher hasn't read yet.
    pub fn is_loading(&self, id: &ThemeId) -> bool {
        self.client.is_loading(id)
    }

    /// A theme picker opened: re-list the library and read every library theme's file that
    /// changed for its preview. Call each frame the picker shows; files already read are not
    /// read again.
    pub fn read_previews(&mut self) {
        let latest = self.client.watch().snapshot();
        let index = self.client.snapshot().index.clone();
        for e in index.iter() {
            let ask = match self.previews.get(&e.id) {
                Some(Preview::Read(stamp, _)) => *stamp != e.stamp,
                Some(Preview::Requested(request, stamp)) if *stamp == e.stamp => match latest.result(*request) {
                    Some(result) => {
                        let record = match result {
                            Ok(OpOutput::Exported(bytes)) => craft_theme_store::parse_theme(bytes).ok().map(Box::new),
                            _ => None,
                        };
                        self.previews.insert(e.id.clone(), Preview::Read(e.stamp, record));
                        false
                    }
                    // Only the latest results are kept: ask again once this one has dropped out.
                    None => latest.results.first().is_some_and(|(oldest, _)| oldest > request),
                },
                _ => true,
            };
            if ask {
                let request = self.client.watch().request(Op::Export(e.id.clone()));
                self.previews.insert(e.id.clone(), Preview::Requested(request, e.stamp));
            }
        }
        self.previews.retain(|id, _| index.iter().any(|e| e.id == *id));
    }

    /// Asks the watcher to list the library again now (a theme picker opened).
    pub fn rescan(&self) {
        self.client.watch().request(Op::Rescan);
    }

    /// The library theme's record, when one is read: the suite's or PhotoCraft's theme in use,
    /// or one read for its preview.
    fn record(&self, id: &ThemeId) -> Option<&ThemeRecord> {
        let snapshot = self.client.snapshot();
        [&snapshot.active, &snapshot.app_theme].into_iter().flatten().find(|r| r.theme.id == *id).or_else(|| match self.previews.get(id) {
            Some(Preview::Read(_, record)) => record.as_deref(),
            _ => None,
        })
    }

    /// The tokens a theme picker previews `id` with, for (Light, Dark): `None` until a library
    /// theme's file is read.
    pub fn preview(&self, id: &ThemeId) -> Option<(Tokens, Tokens)> {
        let theme = craft_theme::builtin(id).cloned().or_else(|| self.record(id).map(|r| r.theme.clone()))?;
        let selection = Selection { theme: id.clone(), mode: Mode::System };
        let own = (ThemeKind::ProLight, ThemeKind::ProMedium);
        Some(resolve_slots(&selection, &|wanted| (*wanted == theme.id).then(|| theme.clone()), own))
    }

    /// A library theme's contrast findings (advisory: its colours are used as they are).
    /// Built-ins have none.
    pub fn diagnostics(&self, id: &ThemeId) -> &[Diagnostic] {
        self.record(id).map_or(&[], |r| &r.diagnostics)
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
