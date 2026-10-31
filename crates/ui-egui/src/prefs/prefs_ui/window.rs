//! The Preferences window's contents on the library's [`PreferencesBody`]: the host builds the
//! pages from the draft in the dialog's `values` field (the engine's preference sections as
//! JSON), and edits go back into that field, so `ui.dialog.set` and `ui.dialog.confirm` keep
//! driving it.

use std::ops::RangeInclusive;

use craft_theme::Rgba;
use craft_ui::controls::{Button, ButtonVariant, ControlSize, TextField};
use craft_ui::edit::EditEvent;
use craft_ui::interface_scale::{Fit, InterfaceScale, InterfaceScaleLabels};
use craft_ui::localized::{Labels as _, LocaleNumbers};
use craft_ui::preferences::{GroupSpec, PageSpec, PrefValue, PreferencesBody, PreferencesHost, PreferencesTexts, RowKind, RowSpec};
use egui::RichText;

use super::appearance::{ThemeChoice, system_theme, theme_choices, theme_slot};
use super::dialogs::{confirm, preference_values, preferences_changed};
use super::fields::{choice_label, ctx_take_pick, field_visible, humanize};
use super::graphics::{adapter_options, gpu_memory_lines, read_at_launch, rendering_mode_options, rendering_mode_value};
use super::{Map, PhotocraftApp, SECTIONS, Tokens, Value, json, prefs};

/// Salt of the body's remembered page, search text and scroll.
const SALT: &str = "preferences";

/// The key of the Graphics block's custom row (it edits nothing).
const GRAPHICS_KEY: &str = "performance.graphics";

/// Photoshop's note under the settings that are read when the app starts.
fn launch_note() -> &'static str {
    tl!("Applies at next launch.")
}

enum Kind {
    Bool(bool),
    Choice { value: String, options: Vec<(String, String)> },
    Color(Rgba),
    Int { value: i64, range: RangeInclusive<i64>, unit: Option<&'static str> },
    Float { value: f64, range: RangeInclusive<f64> },
    Text(String),
    Custom,
}

struct Row {
    key: String,
    label: String,
    keywords: String,
    help: Option<String>,
    step: Option<f64>,
    kind: Kind,
}

struct Group {
    title: Option<String>,
    rows: Vec<Row>,
}

pub(super) struct Page {
    pub(super) id: &'static str,
    title: String,
    custom: bool,
    footnote: Option<String>,
    groups: Vec<Group>,
}

impl Page {
    #[cfg(test)]
    pub(super) fn has_content(&self) -> bool {
        self.custom || self.groups.iter().any(|g| !g.rows.is_empty())
    }
}

impl Row {
    fn new(path: &str, human: &str, kind: Kind) -> Self {
        Self { key: path.to_string(), label: tl!(human).to_string(), keywords: format!("{path} {human}"), help: None, step: None, kind }
    }

    fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

fn group(rows: Vec<Row>) -> Group {
    Group { title: None, rows }
}

/// The generic row of one setting, by the type of its JSON value and the engine's metadata.
/// `None` for values the window does not edit.
fn setting_row(path: &str, key: &str, v: &Value, scale: &[(String, String)]) -> Option<Row> {
    let human = humanize(key);
    let mut row = match v {
        Value::Bool(b) => Row::new(path, &human, Kind::Bool(*b)),
        Value::String(s) if path == "interface.language" => {
            let mut options = vec![("auto".to_string(), tl!("Auto").to_string())];
            options.extend(crate::i18n::Lang::for_picker().into_iter().map(|l| (l.code().to_string(), l.name().to_string())));
            Row::new(path, &human, Kind::Choice { value: s.clone(), options })
        }
        Value::String(s) if path == "interface.uiScale" => Row::new(path, &human, Kind::Choice { value: s.clone(), options: scale.to_vec() }),
        Value::String(s) if prefs::choices(path).is_some() => {
            let options = prefs::choices(path).unwrap_or(&[]).iter().map(|o| (o.to_string(), tl!(&choice_label(o)).to_string())).collect();
            Row::new(path, &human, Kind::Choice { value: s.clone(), options })
        }
        Value::String(s) if prefs::is_color(path) => {
            let [r, g, b] = prefs::parse_hex(s).unwrap_or([128, 128, 128]);
            Row::new(path, &human, Kind::Color(Rgba::rgb(r, g, b)))
        }
        Value::String(s) => Row::new(path, &human, Kind::Text(s.clone())),
        Value::Number(n) => {
            let (lo, hi) = prefs::range(path).unwrap_or((-1e9, 1e9));
            if n.is_u64() || n.is_i64() {
                Row::new(path, &human, Kind::Int { value: n.as_i64().unwrap_or(0), range: lo as i64..=hi as i64, unit: None })
            } else {
                Row::new(path, &human, Kind::Float { value: n.as_f64().unwrap_or(0.0), range: lo..=hi }).step(0.1)
            }
        }
        Value::Array(_) => Row::new(path, &human, Kind::Custom),
        _ => return None,
    };
    if read_at_launch(path) {
        row = row.help(launch_note());
    }
    Some(row)
}

/// The rows of the Performance page that are not generic settings: the rendering mode, the memory
/// limit, what the app renders with now, and the backend and adapter.
fn performance_groups(obj: &Map<String, Value>, generic: Vec<Row>, gpu: &Gpu) -> Vec<Group> {
    let mut groups = Vec::new();
    {
        let note = format!("{}\n{}", tl!("Automatic uses GPU acceleration when available and falls back to CPU rendering on errors."), launch_note());
        let row = Row::new("performance.renderingMode", "Rendering Mode", Kind::Choice { value: rendering_mode_value(obj), options: rendering_mode_options() })
            .help(note);
        groups.push(group(vec![row]));
    }
    {
        let mb = obj.get("memoryUsageMb").and_then(Value::as_i64).unwrap_or(8192);
        let mut help = Vec::new();
        if let Some(ram) = crate::gpu_canvas::physical_memory().filter(|n| *n > 0) {
            let installed = ram / (1 << 20);
            let percent = mb as f64 * 100.0 / (ram as f64 / (1 << 20) as f64);
            help.push(crate::i18n::fmt(
                tl!("Installed RAM: {ram} MB; this limit: {percent}%"),
                &[("ram", &installed.to_string()), ("percent", &format!("{percent:.1}"))],
            ));
        }
        help.push(tl!("Limits each open document's pixels plus History. Oldest undo states are discarded when the limit is exceeded.").to_string());
        help.push(tl!("Applies immediately when saved. Memory is not reserved in advance.").to_string());
        help.push(tl!("With GPU Memory set to Automatic, this also limits the GPU layer budget. That budget is capped at one quarter of installed RAM and, when known, three eighths of the graphics card's memory, with a 512 MB minimum. Raising this limit beyond those caps does not increase the GPU budget.").to_string());
        let row = Row::new("performance.memoryUsageMb", "Per-document RAM limit (MB)", Kind::Int { value: mb, range: 256..=1_048_576, unit: Some("MB") })
            .help(help.join("\n"));
        groups.push(Group { title: Some(tl!("Memory Usage").to_string()), rows: vec![row] });
    }
    groups.push(group(generic));
    // An empty label: the info lines take the whole row under the group's title.
    let mut graphics = Row::new(GRAPHICS_KEY, "", Kind::Custom);
    graphics.label = String::new();
    graphics.keywords = "performance GPU graphics adapter backend".into();
    groups.push(Group { title: Some(tl!("Graphics").to_string()), rows: vec![graphics] });
    let mut advanced = vec![Row::new(
        "performance.gpuBackend",
        "GPU Backend",
        Kind::Choice {
            value: obj.get("gpuBackend").and_then(Value::as_str).unwrap_or("auto").to_string(),
            options: prefs::choices("performance.gpuBackend").unwrap_or(&[]).iter().map(|o| (o.to_string(), tl!(&choice_label(o)).to_string())).collect(),
        },
    )];
    if !prefs::is_hidden("performance.gpuAdapter") {
        let current = obj.get("gpuAdapter").and_then(Value::as_str).unwrap_or("");
        advanced.push(
            Row::new("performance.gpuAdapter", "GPU adapter", Kind::Choice { value: current.to_string(), options: adapter_options(&gpu.adapters, current) })
                .help(launch_note()),
        );
    }
    groups.push(Group { title: Some(tl!("Advanced").to_string()), rows: advanced });
    groups
}

/// What the app renders with now and the adapters it found.
#[derive(Default)]
struct Gpu {
    lines: Vec<String>,
    adapters: Vec<String>,
}

fn gpu_of(app: &PhotocraftApp) -> Gpu {
    let mut lines = app.perf.gpu_info.lines();
    lines.extend(gpu_memory_lines(app));
    Gpu { lines, adapters: app.perf.gpu_info.adapters.clone() }
}

/// The pages of the draft `values`, in `SECTIONS` order. `order` is each section's keys in
/// declaration order.
pub(super) fn pages(values: &Value, order: &Value, gpu_app: Option<&PhotocraftApp>, scale_labels: &InterfaceScaleLabels) -> Vec<Page> {
    let gpu = gpu_app.map(gpu_of).unwrap_or_default();
    SECTIONS
        .iter()
        .map(|&(id, title)| {
            let title = tl!(title).to_string();
            let empty = Map::new();
            let obj = values.get(id).and_then(Value::as_object).unwrap_or(&empty);
            let scale = if id == "interface" { scale_options(obj, scale_labels) } else { Vec::new() };
            let order: Vec<String> = order.get(id).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
            let mut keys: Vec<&String> = order.iter().filter(|k| obj.contains_key(*k)).collect();
            keys.extend(obj.keys().filter(|k| !order.contains(k)));
            let rows: Vec<Row> =
                keys.into_iter().filter(|k| field_visible(id, obj, k, "")).filter_map(|k| setting_row(&format!("{id}.{k}"), k, &obj[k.as_str()], &scale)).collect();
            let footnote =
                (id == "fileHandling").then(|| tl!("0 turns this threshold off; SVG groups nested deeper than 100 levels are always rasterized.").to_string());
            let groups = match id {
                "performance" => performance_groups(obj, rows, &gpu),
                "interface" => interface_groups(obj, rows),
                _ => vec![group(rows)],
            };
            Page { id, title, custom: false, footnote, groups }
        })
        .collect()
}

/// The UI Scaling choices: the library's steps and words, every step the engine accepts.
fn scale_options(obj: &Map<String, Value>, labels: &InterfaceScaleLabels) -> Vec<(String, String)> {
    let current = obj.get("uiScale").and_then(Value::as_str).and_then(InterfaceScale::from_code).unwrap_or_default();
    let lang = obj.get("language").and_then(Value::as_str).unwrap_or("auto");
    let numbers = LocaleNumbers::from_tag(crate::i18n::Lang::from_pref(lang).code());
    InterfaceScale::options(current, labels, &numbers, Fit::default())
}

/// The Interface page in Photoshop's order: Appearance (mode, the two theme rows, Size, the
/// canvas), Presentation, then the check boxes and the remaining options.
fn interface_groups(obj: &Map<String, Value>, rows: Vec<Row>) -> Vec<Group> {
    const APPEARANCE: [&str; 3] = ["canvasColor", "canvasCustomColor", "canvasBorder"];
    const PRESENTATION: [&str; 3] = ["language", "uiFontSize", "uiScale"];
    let text = |k: &str, default: &str| obj.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or(default).to_string();
    let choice = |path: &str, human: &str, key: &str, default: &str, options: [(&str, &str); 3]| {
        let options = options.iter().map(|(id, label)| (id.to_string(), tl!(*label).to_string())).collect();
        Row::new(path, human, Kind::Choice { value: text(key, default), options })
    };
    let mode = choice(
        "interface.appearanceMode",
        "Appearance Mode",
        "appearanceMode",
        "dark",
        [("auto", "Sync with system"), ("dark", "Dark"), ("light", "Light")],
    );
    let size = choice("interface.size", "Size", "size", "suite", [("suite", "Suite default"), ("compact", "Compact"), ("comfortable", "Comfortable")]);
    let theme = |path: &str, human: &str| {
        let mut row = Row::new(path, human, Kind::Custom);
        row.keywords.push_str(" Color Theme");
        row
    };
    let key_of = |r: &Row| r.key.strip_prefix("interface.").unwrap_or(&r.key).to_string();
    let take = |rows: &mut Vec<Row>, names: &[&str]| -> Vec<Row> {
        let mut out = Vec::new();
        for n in names {
            if let Some(i) = rows.iter().position(|r| key_of(r) == *n) {
                out.push(rows.remove(i));
            }
        }
        out
    };
    let mut rest = rows;
    let mut appearance = vec![mode, theme("interface.themeLight", "Light Theme"), theme("interface.themeDark", "Dark Theme"), size];
    appearance.extend(take(&mut rest, &APPEARANCE));
    let presentation = take(&mut rest, &PRESENTATION);
    let (checks, other): (Vec<Row>, Vec<Row>) = rest.into_iter().partition(|r| matches!(r.kind, Kind::Bool(_)));
    let titled = |title: &str, rows| Group { title: Some(tl!(title).to_string()), rows };
    let mut groups = vec![titled("Appearance", appearance), titled("Presentation", presentation)];
    if !checks.is_empty() {
        groups.push(Group { title: Some(tl!("Options").to_string()), rows: checks });
    }
    if !other.is_empty() {
        groups.push(Group { title: None, rows: other });
    }
    groups.retain(|g| !g.rows.is_empty());
    groups
}

fn prefs_texts(lang: crate::i18n::Lang) -> PreferencesTexts {
    PreferencesTexts {
        title: tl!("Preferences").into(),
        search_label: tl!("Search settings…").into(),
        search_placeholder: tl!("Search settings…").into(),
        clear_search: tl!("Clear search").into(),
        pages_label: tl!("Pages").into(),
        no_matches: tl!("No matching settings").into(),
        empty_page: tl!("These settings aren't available in PhotoCraft yet.").into(),
        ok: tl!("OK").into(),
        cancel: tl!("Cancel").into(),
        apply: tl!("Apply").into(),
        reset_section: tl!("Reset Section").into(),
        hex_label: tl!("Color").into(),
        default_hint: tl!("Default").into(),
        numbers: LocaleNumbers::from_tag(lang.code()),
    }
}

pub(super) struct Host<'a> {
    app: &'a mut PhotocraftApp,
    fields: &'a mut Map<String, Value>,
    pages: Vec<Page>,
    gpu: Gpu,
    lang: crate::i18n::Lang,
    system: Option<egui::Theme>,
    /// The library themes the Interface page's theme rows offer.
    themes: Vec<ThemeChoice>,
}

impl<'a> Host<'a> {
    /// The window's host over the dialog's `fields`.
    pub(super) fn new(app: &'a mut PhotocraftApp, fields: &'a mut Map<String, Value>, system: Option<egui::Theme>, scale_labels: &InterfaceScaleLabels) -> Self {
        let lang = crate::i18n::Lang::from_pref(fields.get("values").and_then(|v| v.pointer("/interface/language")).and_then(Value::as_str).unwrap_or("auto"));
        let values = fields.get("values").cloned().unwrap_or(Value::Null);
        let order = fields.get("__order").cloned().unwrap_or(Value::Null);
        let pages = pages(&values, &order, Some(app), scale_labels);
        let gpu = gpu_of(app);
        let themes = theme_choices(app);
        Host { app, fields, pages, gpu, lang, system, themes }
    }

    fn section_mut(&mut self, section: &str) -> Option<&mut Map<String, Value>> {
        self.fields.get_mut("values")?.get_mut(section)?.as_object_mut()
    }

    fn value(&self, key: &str) -> Value {
        let (section, name) = key.split_once('.').unwrap_or((key, ""));
        self.fields.get("values").and_then(|v| v.get(section)).and_then(|s| s.get(name)).cloned().unwrap_or(Value::Null)
    }
}

impl PreferencesHost for Host<'_> {
    fn texts(&self) -> PreferencesTexts {
        prefs_texts(self.lang)
    }

    fn pages(&self) -> Vec<PageSpec<'_>> {
        self.pages
            .iter()
            .map(|p| {
                if p.custom {
                    return PageSpec::custom(p.id, &p.title);
                }
                let groups = p
                    .groups
                    .iter()
                    .map(|g| {
                        let rows = g
                            .rows
                            .iter()
                            .map(|r| {
                                let kind = match &r.kind {
                                    Kind::Bool(b) => RowKind::Bool(*b),
                                    Kind::Choice { value, options } => {
                                        RowKind::Choice { value, options: options.iter().map(|(id, label)| (id.as_str(), label.as_str())).collect() }
                                    }
                                    Kind::Color(c) => RowKind::Color(*c),
                                    Kind::Int { value, range, unit } => RowKind::Int { value: *value, range: range.clone(), unit: *unit },
                                    Kind::Float { value, range } => RowKind::Float { value: *value, range: range.clone(), decimals: 3, unit: None },
                                    Kind::Text(t) => RowKind::Text(t),
                                    Kind::Custom => RowKind::Custom,
                                };
                                let mut spec = RowSpec::new(&r.key, &r.label, kind).keywords(&r.keywords);
                                if let Some(help) = &r.help {
                                    spec = spec.help(help);
                                }
                                if let Some(step) = r.step {
                                    spec = spec.step(step);
                                }
                                spec
                            })
                            .collect();
                        GroupSpec::new(g.title.as_deref(), rows)
                    })
                    .collect();
                let mut page = PageSpec { id: p.id, title: &p.title, groups, footnote: None, custom: false, keywords: "" };
                page.footnote = p.footnote.as_deref();
                page
            })
            .collect()
    }

    fn set(&mut self, key: &str, value: PrefValue) {
        let Some((section, name)) = key.split_once('.') else { return };
        let json = match value {
            PrefValue::Bool(b) => json!(b),
            PrefValue::Int(i) => json!(i),
            PrefValue::Float(f) => json!(f),
            PrefValue::Text(s) | PrefValue::Choice(s) => json!(s),
            PrefValue::Color(c) => json!(format!("#{:02x}{:02x}{:02x}", c.0[0], c.0[1], c.0[2])),
        };
        let Some(obj) = self.section_mut(section) else { return };
        // The rendering mode also keeps the legacy GPU flag in step.
        if key == "performance.renderingMode" {
            obj.insert("useGpu".into(), json!(json != "cpu"));
        }
        obj.insert(name.to_string(), json);
    }

    fn custom_row(&mut self, ui: &mut egui::Ui, key: &str) {
        let t = Tokens::get(ui.ctx());
        if key == GRAPHICS_KEY {
            ui.vertical(|ui| {
                for line in &self.gpu.lines {
                    ui.label(RichText::new(line).color(t.text_dim));
                }
                ui.label(RichText::new(launch_note()).color(t.text_faint));
            });
            return;
        }
        if let Some(dark) = match key {
            "interface.themeLight" => Some(false),
            "interface.themeDark" => Some(true),
            _ => None,
        } {
            let system = self.system;
            let themes = std::mem::take(&mut self.themes);
            if let Some(obj) = self.section_mut("interface") {
                theme_slot(ui, obj, &themes, system, dark);
            }
            self.themes = themes;
            return;
        }
        let Some((section, name)) = key.split_once('.') else { return };
        let v = self.value(key);
        let lang = self.lang;
        let Some(obj) = self.section_mut(section) else { return };
        match &v {
            Value::Array(_) if key == "tools.pressureCurve" => {
                if let Some(next) = crate::pressure_curve_ui::editor(ui, &v) {
                    obj.insert(name.to_string(), next);
                }
            }
            Value::Array(items) if name == "disks" => {
                let mut items = items.clone();
                ui.vertical(|ui| {
                    for (n, d) in items.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            let mut on = d.get("enabled").and_then(Value::as_bool).unwrap_or(false);
                            let mut path = d.get("path").and_then(Value::as_str).unwrap_or("").to_string();
                            let _ = craft_ui::controls::Checkbox::bool(tl!("Enabled"), &mut on).show(ui);
                            let edit = TextField::new(("scratch-disk-path", n), tl!("Path"), path.as_str()).show(ui);
                            if let Some(EditEvent::Update { value, .. } | EditEvent::Set { value, .. } | EditEvent::Commit { value, .. }) = edit.event {
                                path = value;
                            }
                            *d = json!({"enabled": on, "path": path});
                        });
                    }
                    if ui.add(Button::new(tl!("Add disk")).variant(ButtonVariant::Secondary).size(ControlSize::Sm)).clicked() {
                        items.push(json!({"enabled": true, "path": ""}));
                    }
                });
                obj.insert(name.to_string(), Value::Array(items));
            }
            Value::Array(items) => {
                ui.label(RichText::new(crate::i18n::trn(lang, items.len() as u64, "{n} item", "{n} items")).color(t.text_faint));
                if !items.is_empty() && ui.add(Button::new(tl!("Clear")).variant(ButtonVariant::Secondary).size(ControlSize::Sm)).clicked() {
                    obj.insert(name.to_string(), json!([]));
                }
            }
            _ => {}
        }
    }

    fn custom_page(&mut self, _ui: &mut egui::Ui, _page: &str) {}

    fn differs(&self) -> bool {
        preferences_changed(self.app, self.fields)
    }

    fn apply(&mut self) -> Result<(), String> {
        confirm(self.app, self.fields)?;
        // The validated values are the next draft, including any normalisation by `prefs.set`.
        self.fields.insert("values".into(), Value::Object(preference_values(self.app.session.prefs())));
        Ok(())
    }

    fn cancel(&mut self) {}

    fn reset_page(&mut self, page: &str) {
        let Some(mut def) = prefs::Preferences::default().get(page) else { return };
        // The favourite fonts are the user's library, not a Type setting: kept, as
        // `prefs.reset {"path": "type"}` keeps them.
        if page == "type"
            && let Some(favorites) = self.value("type.favoriteFonts").as_array().map(|_| self.value("type.favoriteFonts"))
            && let Some(def) = def.as_object_mut()
        {
            def.insert("favoriteFonts".into(), favorites);
        }
        if let Some(values) = self.fields.get_mut("values").and_then(Value::as_object_mut) {
            values.insert(page.to_string(), def);
        }
    }
}

/// Draws the Preferences body. Returns `Some(true)` when OK answered it (already applied) and
/// `Some(false)` when Cancel did.
pub(super) fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &mut Map<String, Value>, interactive: bool) -> Option<bool> {
    let system = system_theme(app, ui.ctx());
    let mut host = Host::new(app, f, system, &InterfaceScaleLabels::of(ui));
    let body_id = PreferencesBody::id(ui, SALT);
    // The dialog's `section` and `__search` fields are the page and the search text: automation
    // sets them, and the body's own state follows a change made outside it.
    let mut state = PreferencesBody::state(ui.ctx(), body_id);
    let mut moved = false;
    for (field, slot) in [("section", &mut state.selected), ("__search", &mut state.search)] {
        if let Some(want) = host.fields.get(field).and_then(Value::as_str)
            && want != slot.as_str()
        {
            *slot = want.to_string();
            moved = true;
        }
    }
    if moved {
        PreferencesBody::set_state(ui.ctx(), body_id, state);
    }
    let out = PreferencesBody::new(SALT).active(interactive).show(ui, &mut host);
    let state = PreferencesBody::state(ui.ctx(), body_id);
    f_sync(host.fields, &state.search, out.page.as_deref());
    // A colour swatch was clicked: open the Color Picker on that value (#2144).
    let picked = out.pick_color.map(|key| {
        let human = humanize(key.split_once('.').map_or(key.as_str(), |(_, k)| k));
        (key, human)
    });
    for (path, name) in picked.into_iter().chain(ctx_take_pick(ui)) {
        crate::color_picker_ui::request_field(f, &format!("/values/{}", path.replace('.', "/")), &crate::color_picker_ui::title_for(tl!(&name)));
    }
    match out.closed {
        Some(craft_ui::preferences::Closed::Ok) => {
            f.insert(super::APPLIED.into(), json!(true));
            Some(true)
        }
        Some(craft_ui::preferences::Closed::Cancel) => Some(false),
        None => None,
    }
}

/// Writes the body's page and search text back to the dialog's fields.
fn f_sync(fields: &mut Map<String, Value>, search: &str, page: Option<&str>) {
    fields.insert("__search".into(), json!(search));
    if let Some(page) = page {
        fields.insert("section".into(), json!(page));
    }
}
