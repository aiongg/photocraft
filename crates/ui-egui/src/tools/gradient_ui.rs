//! Gradient tool, live mode (Photoshop 2023+ "Gradient" vs "Classic gradient"): a drag makes a
//! Gradient Fill layer (`gradient.fill.create`), and with a gradient fill layer selected the canvas
//! shows its widget: start and end handles (drag to move, ⇧ snaps the angle to 45°), the colour
//! stops along the line (drag to move, drag off the line to delete, double-click for the Color
//! Picker), midpoint diamonds between them, a click on the line adds a stop, and dragging the line
//! moves the whole gradient. A drag elsewhere redraws the selected gradient. Radial and Angle
//! gradients show their circle, Diamond its square, Reflected its mirrored half.
//!
//! Every gesture previews live through the regular canvas path (a shallow copy of the document
//! with the edited fill, so the GPU canvas just re-runs its gradient pass) and commits one command
//! on release: one history step. The Properties panel's stops editor ([`properties`]) previews
//! through the same path.

use craft::draw::ui::gradient::GradientSwatch;
use craft::draw::ui::{
    ColorStop, GradientExtent, GradientLine, GradientLineNames, GradientLineStyle, GradientOverlay, GradientStops, LineGradient, LinePart, LinePress,
    LineRelease, StopRef, StopRows, StopRules, TaggedStop,
};
use craft::theme::Srgba;
use craft::ui::canvas::CanvasView;
use craft::ui::canvas::kurbo::Point;
use craft::ui::controls::{Button, ButtonVariant, ControlSize};
use craft::ui::primitives::{Content, Text};
use std::sync::Arc;
use std::sync::OnceLock;

use egui::{Color32, Pos2, Rect, pos2, vec2};
use photocraft_compose::gradient_fill as gf;
use photocraft_doc::{Document, Fill, GradientStyle, Layer, LayerContent, LayerId};
use photocraft_engine::gradient_fill_cmds as cmds;
use photocraft_engine::presets::gradients as gradient_presets;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::canvas::{ToolEvent, ViewXform};
use crate::state::Tool;
use crate::widgets;

#[cfg(test)]
mod editor_tests;
mod properties_panel;
pub(crate) use properties_panel::style_icon;
pub(crate) mod stops_bar;

use stops_bar::BarAction;

/// Id of the temporary layer previewing a new gradient while it is drawn.
pub const PREVIEW_LAYER: LayerId = LayerId(u64::MAX - 78);
/// Tag of this module's canvas preview keys.
const TAG: u64 = 1 << 58;

#[derive(Clone, Debug)]
enum Drag {
    /// Drawing a new gradient, or redrawing the selected one.
    Draw { from: [f32; 2], to: [f32; 2], redraw: Option<LayerId> },
    /// Editing the selected gradient: the library overlay's press, the stop or midpoint it
    /// selects, and the command (and params) to commit on release.
    Edit { layer: LayerId, press: LinePress, selected: Option<TaggedStop>, commit: Option<(&'static str, Value)> },
}

/// Live-gradient interaction state (not saved).
#[derive(Default)]
pub struct LiveGradient {
    drag: Option<Drag>,
    /// The release being delivered ends egui's double click (set by the canvas before the event).
    pub(crate) double_click: bool,
    /// The parts the last two presses hit (newest last): a double click asks a colour only when
    /// both of its clicks hit the same part.
    pressed: [Option<LinePart>; 2],
    /// Pending edit from the Properties panel's stops editor (previewed like a canvas drag).
    panel: Option<(LayerId, &'static str, Value)>,
    /// Pending edit from the Gradient Editor window's current-gradient strip
    /// (`gradient.presets.stop`, previewed the same way).
    preset_stop: Option<Value>,
    /// Cached preview document by key.
    preview: Option<(u64, Arc<Document>)>,
}

/// Whether the Gradient tool is in its live (Gradient Fill layer) mode.
/// Painting into a layer mask, an alpha channel or the Quick Mask stays classic (pixels), except
/// that a selected gradient fill layer is always edited live (selecting it targets its mask).
pub fn live_mode(app: &PhotocraftApp) -> bool {
    if app.active_tool() != Tool::Gradient || app.ui.tool_options.gradient_classic {
        return false;
    }
    match crate::canvas::paint_target(app).as_str() {
        Some("pixels") => true,
        Some("mask") => active_gradient(app).is_some(),
        _ => false,
    }
}

/// The active layer when it is a gradient fill layer: (layer, its frame, the canvas).
fn active_gradient(app: &PhotocraftApp) -> Option<(Layer, Rect32, photocraft_geom::Rect)> {
    let st = app.session.active()?;
    let l = st.doc.layer(st.active_layer?)?;
    if !matches!(l.content, LayerContent::Fill(Fill::Gradient { .. })) || !l.visible {
        return None;
    }
    let canvas = st.doc.bounds();
    Some((l.clone(), photocraft_compose::fill_frame(l, canvas), canvas))
}

type Rect32 = photocraft_geom::Rect;

fn fill_of(l: &Layer) -> Option<&Fill> {
    match &l.content {
        LayerContent::Fill(f @ Fill::Gradient { .. }) => Some(f),
        _ => None,
    }
}

/// The fill an in-progress edit shows (`None` when it can't apply, e.g. a stale index).
fn edited_fill(app: &PhotocraftApp, layer: &Layer, canvas: Rect32, cmd: &str, p: &Value) -> Option<Fill> {
    let f = fill_of(layer)?;
    let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
    let edited = match cmd {
        cmds::SET => cmds::apply_set(layer, f, canvas, p, fg, bg).ok(),
        cmds::STOP => cmds::apply_stop(f, p, fg, bg).ok(),
        _ => None,
    }?;
    // As the command commits it: colours in the document's mode.
    Some(match app.session.active() {
        Some(st) => edited.in_mode(st.doc.mode),
        None => edited,
    })
}

/// Options-bar params of a new live gradient.
fn create_params(app: &PhotocraftApp, from: [f32; 2], to: [f32; 2]) -> Value {
    let o = &app.ui.tool_options;
    json!({"from": from, "to": to, "style": o.gradient_style, "reverse": o.gradient_reverse, "dither": o.gradient_dither, "opacity": o.fill_opacity, "mode": o.gradient_blend_mode.label()})
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The library's kit for a gradient fill: the line between its handles and its stops in location
/// order, as the overlay's value, with the fill's reverse flag and extent. Each stop's key is its
/// position in the fill's own list + 1 (an added stop has key 0), so an
/// edit's result maps back to the stop it moves wherever a drag reorders it.
fn line_gradient(f: &Fill, frame: Rect32) -> Option<(LineGradient, bool, GradientExtent)> {
    let Fill::Gradient { stops, midpoints, reverse, style, .. } = f else { return None };
    let (start, end) = gf::fill_handles(f, frame)?;
    let mut sorted: Vec<(f32, [f32; 3], u64)> = stops.iter().enumerate().map(|(i, (t, c))| (*t, c.to_rgb(), i as u64 + 1)).collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let color = sorted
        .iter()
        .enumerate()
        .map(|(i, (t, rgb, key))| ColorStop {
            location: *t,
            color: Srgba { r: rgb[0].clamp(0.0, 1.0), g: rgb[1].clamp(0.0, 1.0), b: rgb[2].clamp(0.0, 1.0), a: 1.0 },
            midpoint: midpoints.get(i).copied().unwrap_or(0.5),
            tag: *key,
        })
        .collect();
    let point = |p: [f32; 2]| Point::new(f64::from(p[0]), f64::from(p[1]));
    let extent = match style {
        GradientStyle::Radial | GradientStyle::Angle => GradientExtent::Circle,
        GradientStyle::Diamond => GradientExtent::Square,
        GradientStyle::Reflected => GradientExtent::Mirror,
        GradientStyle::Linear => GradientExtent::None,
    };
    Some((LineGradient { line: GradientLine { start: point(start), end: point(end) }, stops: GradientStops::new(color) }, *reverse, extent))
}

/// The library's overlay over a gradient's `value`: colour stops only, as PhotoCraft's fills have no
/// opacity stops.
fn overlay<'a>(value: &'a LineGradient, reverse: bool, extent: GradientExtent) -> GradientOverlay<'a> {
    GradientOverlay::new("gradient-tool", &value.line, &value.stops, GradientLineNames::default())
        .rules(StopRules { rows: StopRows::ColorOnly, ..StopRules::default() })
        .reverse(reverse)
        .extent(extent)
}

/// The library's sizes (`gradient_line.*`): the same in every theme, so the pointer reads them
/// without a context.
fn reach() -> &'static GradientLineStyle {
    static STYLE: OnceLock<GradientLineStyle> = OnceLock::new();
    STYLE.get_or_init(|| GradientOverlay::style(&craft::ui::theme::current(&egui::Context::default())))
}

/// The command an edit of the line's gradient commits: `after` is what the drag of `press` made
/// of the gradient at the press.
fn commit_of(layer: LayerId, press: &LinePress, before: &LineGradient, after: &LineGradient) -> Option<(&'static str, Value)> {
    let id = layer.0;
    let xy = |p: Point| [p.x as f32, p.y as f32];
    match press.part() {
        LinePart::Start => Some((cmds::SET, json!({"layer": id, "from": xy(after.line.start)}))),
        LinePart::End => Some((cmds::SET, json!({"layer": id, "to": xy(after.line.end)}))),
        LinePart::Line => Some((cmds::SET, json!({"layer": id, "from": xy(after.line.start), "to": xy(after.line.end)}))),
        LinePart::Stop(i) if press.torn() => Some((cmds::STOP, json!({"layer": id, "action": "delete", "index": i}))),
        LinePart::Stop(i) => {
            let moved = after.stops.color.iter().find(|s| Some(s.tag) == before.tag_of(StopRef::Color(i)))?;
            Some((cmds::STOP, json!({"layer": id, "action": "move", "index": i, "location": moved.location})))
        }
        LinePart::Midpoint(i) => {
            let mid = after.stops.color.get(i)?.midpoint;
            Some((cmds::STOP, json!({"layer": id, "action": "midpoint", "index": i, "location": mid})))
        }
    }
}

/// Pointer input for the Gradient tool in live mode. Returns true when consumed.
pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: egui::Modifiers) -> bool {
    if !live_mode(app) {
        app.gradient.drag = None;
        return false;
    }
    // ⌘ is the Move tool (`hold_keys::cmd_moves`): a press with it held drags a guide or the layer
    // (#6288). A drag already under way keeps its events.
    if app.gradient.drag.is_none() && crate::canvas::event_tool(app, mods) != Tool::Gradient {
        return false;
    }
    match ev {
        ToolEvent::Down { x, y, .. } => {
            let p = [x as f32, y as f32];
            let active = active_gradient(app);
            let press = active.as_ref().and_then(|(l, frame, _)| {
                let (value, reverse, extent) = line_gradient(fill_of(l)?, *frame)?;
                let (view, at) = screen_point(app, p)?;
                overlay(&value, reverse, extent).press(&view, reach(), at, mods)
            });
            let [_, last] = app.gradient.pressed;
            app.gradient.pressed = [last, press.as_ref().map(LinePress::part)];
            app.gradient.drag = Some(match (press, active) {
                (Some(press), Some((l, ..))) => Drag::Edit { layer: l.id, selected: press.selects(), press, commit: None },
                (_, active) => Drag::Draw { from: p, to: p, redraw: active.map(|a| a.0.id) },
            });
        }
        ToolEvent::Move { x, y, .. } => drag_to(app, [x as f32, y as f32], mods),
        ToolEvent::Up { x, y } => {
            drag_to(app, [x as f32, y as f32], mods);
            finish(app, [x as f32, y as f32]);
        }
    }
    true
}

/// Document point `p` in the library's view, and where it is on screen.
fn screen_point(app: &PhotocraftApp, p: [f32; 2]) -> Option<(CanvasView, Pos2)> {
    let view = ViewXform::library_view(app);
    let at = view.to_screen(Point::new(f64::from(p[0]), f64::from(p[1])));
    (at.x.is_finite() && at.y.is_finite()).then_some((view, at))
}

fn drag_to(app: &mut PhotocraftApp, p: [f32; 2], mods: egui::Modifiers) {
    let active = active_gradient(app);
    let screen = screen_point(app, p);
    let Some(drag) = app.gradient.drag.as_mut() else { return };
    match drag {
        // ⇧: 45° gradient angles, the same snap as the classic drag (stroke_constraint.rs).
        Drag::Draw { from, to, .. } => {
            *to = if mods.shift {
                let c = crate::stroke_constraint::snap45([f64::from(from[0]), f64::from(from[1])], [f64::from(p[0]), f64::from(p[1])]);
                [c[0] as f32, c[1] as f32]
            } else {
                p
            }
        }
        Drag::Edit { layer, press, selected, commit } => {
            let Some((value, reverse, extent)) = active.as_ref().filter(|a| a.0.id == *layer).and_then(|(l, frame, _)| line_gradient(fill_of(l)?, *frame))
            else {
                return;
            };
            let Some((view, at)) = screen else { return };
            let track = overlay(&value, reverse, extent).track(&view, reach(), press, at, mods);
            if let Some(after) = &track.state {
                *commit = commit_of(*layer, press, &value, after);
            }
            if let Some(now) = track.selected {
                *selected = now;
            }
        }
    }
}

fn finish(app: &mut PhotocraftApp, at: [f32; 2]) {
    let Some(drag) = app.gradient.drag.take() else { return };
    let zoom = app.point_zoom().max(0.01);
    match drag {
        Drag::Draw { from, to, redraw } => {
            if dist(from, to) * zoom < 2.0 {
                return;
            }
            let _ = match redraw {
                Some(id) => app.run(cmds::SET, json!({"layer": id.0, "from": from, "to": to})),
                None => {
                    let p = create_params(app, from, to);
                    app.run(cmds::CREATE, p)
                }
            };
        }
        Drag::Edit { layer, press, commit, .. } => {
            let Some((value, reverse, extent)) = active_gradient(app).filter(|a| a.0.id == layer).and_then(|(l, frame, _)| line_gradient(fill_of(&l)?, frame))
            else {
                return;
            };
            let Some((view, _)) = screen_point(app, at) else { return };
            match overlay(&value, reverse, extent).release(&view, &press) {
                LineRelease::Commit(_) => {
                    if let Some((cmd, p)) = commit {
                        let _ = app.run(cmd, p);
                    }
                }
                // A click on the line adds a colour stop there.
                LineRelease::Add { value: added, selected: TaggedStop { stop: StopRef::Color(i), .. } } => {
                    if let Some(stop) = added.stops.color.get(i) {
                        let _ = app.run(cmds::STOP, json!({"layer": layer.0, "action": "add", "location": stop.location}));
                    }
                }
                LineRelease::Add { .. } => {}
                LineRelease::Nothing => click(app, layer, press.part(), at),
            }
        }
    }
}

/// A click (no drag) on the widget: a double-click on a stop (or on an end handle, for the stop
/// there) opens the Color Picker.
fn click(app: &mut PhotocraftApp, layer: LayerId, part: LinePart, at: [f32; 2]) {
    let [first, second] = app.gradient.pressed;
    if !std::mem::take(&mut app.gradient.double_click)
        || first != second
        || second != Some(part)
        || !matches!(part, LinePart::Stop(_) | LinePart::Start | LinePart::End)
    {
        return;
    }
    let Some((l, frame, _)) = active_gradient(app) else { return };
    let Some((value, reverse, extent)) = fill_of(&l).and_then(|f| line_gradient(f, frame)) else { return };
    let Some((view, point)) = screen_point(app, at) else { return };
    if let Some(request) = overlay(&value, reverse, extent).double_click(&view, reach(), point) {
        // A click reorders nothing: the request's index is the stop's place in the command's order.
        edit_stop_color(app, layer, request.index);
    }
}

/// Opens the Color Picker on colour stop `i`; OK runs `gradient.fill.stop` (one history step).
pub fn edit_stop_color(app: &mut PhotocraftApp, layer: LayerId, i: usize) {
    let Some(st) = app.session.active() else { return };
    let Some(Fill::Gradient { stops, .. }) = st.doc.layer(layer).and_then(fill_of) else { return };
    let mut sorted = stops.clone();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, c)) = sorted.get(i) else { return };
    crate::color_picker_ui::open_for_command(
        app,
        "Color Picker (Stop Color)",
        c.to_rgb(),
        cmds::STOP,
        json!({"layer": layer.0, "action": "color", "index": i}),
    );
}

fn preview_key(revision: u64, what: &str) -> u64 {
    let h = what.bytes().fold(revision.wrapping_mul(0x9e37_79b9_7f4a_7c15), |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    (h & (TAG - 1)) | TAG
}

/// The document to show while a gradient is drawn or edited (`canvas::display_doc`).
pub fn display_doc(app: &mut PhotocraftApp, idx: usize) -> Option<(Arc<Document>, u64)> {
    if app.session.active_index() != Some(idx) {
        return None;
    }
    let st = app.session.active()?;
    let (doc, revision) = (st.doc.clone(), st.revision);
    let edit: Option<(LayerId, &'static str, Value)> = match &app.gradient.drag {
        Some(Drag::Draw { from, to, redraw }) if live_mode(app) && dist(*from, *to) >= 1.0 => match redraw {
            Some(id) => Some((*id, cmds::SET, json!({"from": from, "to": to}))),
            None => Some((PREVIEW_LAYER, cmds::CREATE, create_params(app, *from, *to))),
        },
        Some(Drag::Edit { layer, commit: Some((cmd, p)), .. }) if live_mode(app) => Some((*layer, *cmd, p.clone())),
        _ => app.gradient.panel.clone(),
    };
    let Some((layer, cmd, p)) = edit else {
        app.gradient.preview = None;
        return None;
    };
    let key = preview_key(revision, &format!("{}{cmd}{p}", layer.0));
    if let Some((k, d)) = &app.gradient.preview
        && *k == key
    {
        return Some((d.clone(), key));
    }
    let mut shown = (*doc).clone();
    if cmd == cmds::CREATE {
        let mut l = cmds::new_layer(&app.session, &doc, &p).ok()?;
        l.id = PREVIEW_LAYER;
        // Where the committed layer will go, so the preview doesn't jump on release.
        let active = app.session.active().and_then(|s| s.active_layer);
        photocraft_engine::layer_place::insert_new(&mut shown, active, l, photocraft_engine::layer_place::Place::AutoEditable);
    } else {
        let l = doc.layer(layer)?;
        let f = edited_fill(app, l, doc.bounds(), cmd, &p)?;
        shown.layer_mut(layer)?.content = LayerContent::Fill(f);
    }
    let shown = Arc::new(shown);
    app.gradient.preview = Some((key, shown.clone()));
    Some((shown, key))
}

/// The stop or midpoint a drag selects and the end it holds, as the overlay draws them.
type Marks = (Option<StopRef>, Option<LinePart>);

/// The fill and frame the widget shows: the one being drawn or edited, else the selected one.
fn shown_fill(app: &PhotocraftApp) -> Option<(Fill, Rect32, Marks)> {
    let st = app.session.active()?;
    let canvas = st.doc.bounds();
    match &app.gradient.drag {
        Some(Drag::Draw { from, to, redraw: None }) => {
            let l = cmds::new_layer(&app.session, &st.doc, &create_params(app, *from, *to)).ok()?;
            let f = fill_of(&l)?.clone();
            Some((f, photocraft_compose::fill_frame(&l, canvas), (None, Some(LinePart::End))))
        }
        Some(Drag::Draw { from, to, redraw: Some(id) }) => {
            let l = st.doc.layer(*id)?;
            let f = edited_fill(app, l, canvas, cmds::SET, &json!({"from": from, "to": to}))?;
            Some((f, photocraft_compose::fill_frame(l, canvas), (None, Some(LinePart::End))))
        }
        Some(Drag::Edit { layer, press, selected, commit }) => {
            let l = st.doc.layer(*layer)?;
            let f = match commit {
                Some((cmd, p)) => match edited_fill(app, l, canvas, cmd, p) {
                    Some(f) => f,
                    None => fill_of(l)?.clone(),
                },
                None => fill_of(l)?.clone(),
            };
            let held = matches!(press.part(), LinePart::Start | LinePart::End).then_some(press.part());
            Some((f, photocraft_compose::fill_frame(l, canvas), (selected.map(|t| t.stop), held)))
        }
        None => {
            let (l, frame, _) = active_gradient(app)?;
            Some((fill_of(&l)?.clone(), frame, (None, None)))
        }
    }
}

/// Draws the live gradient widget over the canvas.
pub fn draw_overlay(app: &PhotocraftApp, painter: &egui::Painter, xf: &ViewXform) {
    if !live_mode(app) {
        return;
    }
    let Some((fill, frame, (selected, marked))) = shown_fill(app) else { return };
    let Some((value, reverse, extent)) = line_gradient(&fill, frame) else { return };
    let style = GradientOverlay::style(&craft::ui::theme::current(painter.ctx()));
    overlay(&value, reverse, extent).paint(painter, &style, &xf.canvas_view(), &value, selected, marked);
}

// ---------------------------------------------------------------- options bar

/// Side, in pixels, of a gradient fill layer's thumbnail texture.
const THUMB_PX: u32 = 64;

/// Pixels of a gradient fill's Layers-panel thumbnail: the fill as it lays out (style, angle,
/// scale, offset, reverse, stops, midpoints and opacity) in a square, as Photoshop shows it.
/// `None` for other fills.
pub fn thumbnail_image(f: &Fill) -> Option<egui::ColorImage> {
    let Fill::Gradient { .. } = f else { return None };
    let n = THUMB_PX as i32;
    let square = photocraft_geom::Rect::new(0, 0, n, n);
    // No dither: it is invisible at this size and would only add noise.
    let mut plain = f.clone();
    if let Fill::Gradient { dither, .. } = &mut plain {
        *dither = false;
    }
    let px = gf::render(&plain, square, square);
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let rgba: Vec<Color32> = px.iter().map(|c| Color32::from_rgba_unmultiplied(q(c[0]), q(c[1]), q(c[2]), q(c[3]))).collect();
    Some(egui::ColorImage::new([THUMB_PX as usize; 2], rgba))
}

/// Paints a gradient fill layer's inset thumbnail and fill marker into `rect`, over a
/// checkerboard (opacity stops show through). Cached until its fill changes; `false` for other fills.
pub fn paint_thumbnail(ui: &egui::Ui, layer: LayerId, f: &Fill, rect: Rect) -> bool {
    if !matches!(f, Fill::Gradient { .. }) {
        return false;
    }
    let ctx = ui.ctx();
    let key = egui::Id::unique(("gradient-thumb", layer.0));
    let cached: Option<(Fill, egui::TextureHandle)> = ctx.data(|d| d.get_temp(key));
    let tex = match cached {
        Some((cf, t)) if cf == *f => t,
        _ => {
            let Some(img) = thumbnail_image(f) else { return false };
            let t = ctx.load_texture(format!("gradient-thumb-{}", layer.0), img, egui::TextureOptions::LINEAR);
            ctx.data_mut(|d| d.insert_temp(key, (f.clone(), t.clone())));
            t
        }
    };
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        crate::solid_fill_ui::thumbnail_frame(ui, rect, |swatch| {
            widgets::checker(p, swatch, 3.0);
            p.image(tex.id(), swatch, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        });
    }
    true
}

/// Options-bar swatch of the current gradient. Both live and classic modes use the same
/// engine-owned preset (#1651). Clicking it opens the Gradient Editor window, as Photoshop's
/// swatch does. Returns whether it was clicked.
pub fn preset_swatch(app: &mut PhotocraftApp, ui: &mut egui::Ui) -> bool {
    let Some(fill) = preset_fill(app) else { return false };
    let stops = stops_bar::stops_of(&fill);
    // The swatch is a button named for accessibility (and so tests and agents can find it).
    GradientSwatch::new("gradient-picker", tl!("Gradient picker"), &stops)
        .width(96.0)
        .size(ControlSize::Sm)
        .has_popup(true)
        .show(ui)
        .on_hover_text(tl!("Click to edit the gradient"))
        .clicked()
}

/// The selected layer when it is a gradient fill layer. Unlike [`active_gradient`] it does not
/// require the layer to be visible: the editor edits it either way.
fn selected_gradient(app: &PhotocraftApp) -> Option<Layer> {
    let st = app.session.active()?;
    let l = st.doc.layer(st.active_layer?)?;
    fill_of(l)?;
    Some(l.clone())
}

/// The Gradient Editor window the options-bar swatch opens, standing in for Photoshop's Gradient
/// Editor. A selected gradient fill layer gets a combined ramp, with opacity above and colour
/// below, selected-stop fields and Gradient Options. Without a gradient fill layer, the same
/// stop controls edit the tool's current gradient and offer saving it as a preset.
pub fn editor_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    use craft::ui::dialog::{Body, Dialog, DialogState, InitialFocus, Outcome, Resize, ResizeMax};

    let state_id = egui::Id::unique("gradient-editor-state");
    if !app.ui.panels.gradient_editor {
        // The next open starts afresh.
        ctx.data_mut(|d| d.remove_temp::<DialogState>(state_id));
        return;
    }
    let mut state = ctx.data(|d| d.get_temp::<DialogState>(state_id)).unwrap_or_default();
    let mut show_panel = false;
    // Resolved before the closure so `app` is borrowed only once inside it.
    let layer = selected_gradient(app);
    let shown = Dialog::new("gradient-editor", tl!("Gradient Editor"))
        .modeless()
        .resizable(Resize { default: vec2(330.0, 440.0), min: vec2(280.0, 240.0), max: ResizeMax::Window })
        .body(Body::Scroll)
        .initial_focus(InitialFocus::None)
        .show(ctx, &mut state, &mut (), |ui, ()| match &layer {
            Some(l) => properties_panel::editor(app, ui, l),
            None => {
                // Photoshop's editor edits the gradient the tool paints with, so with no gradient
                // fill layer around the strip edits the current gradient in place.
                ui.add(Text::new(tl!("The gradient the Gradient tool paints with")).content(Content::Muted));
                ui.add_space(craft::ui::theme::scale::Space::S6.px());
                preset_stops_editor(app, ui);
                ui.add_space(craft::ui::theme::scale::Space::S6.px());
                ui.horizontal(|ui| {
                    if ui
                        .add(Button::new(tl!("Save As Preset")).variant(ButtonVariant::Secondary))
                        .on_hover_text(tl!("Adds the current gradient to the Gradients panel"))
                        .clicked()
                    {
                        let _ = app.run("gradient.presets.new", json!({}));
                    }
                    show_panel = ui.add(Button::new(tl!("Gradients")).variant(ButtonVariant::Secondary)).clicked();
                });
            }
        });
    if show_panel {
        let _ = crate::preset_panels::menu(app, "window.panel.gradients", &Value::Null);
    }
    if shown.outcome == Some(Outcome::Close) {
        app.ui.panels.gradient_editor = false;
    }
    ctx.data_mut(|d| d.insert_temp(state_id, state));
}

/// Live mode: style, reverse, dither and blend mode changes in the options bar also edit the selected
/// gradient fill layer (one history step each), as in Photoshop.
pub fn options_changed(app: &mut PhotocraftApp, before: &crate::state::ToolOptions) {
    let o = app.ui.tool_options.clone();
    if app.ui.tool != Tool::Gradient || o.gradient_classic {
        return;
    }
    let Some((l, ..)) = active_gradient(app) else { return };
    if o.gradient_blend_mode != before.gradient_blend_mode {
        let _ = app.run("layer.setProps", json!({"layer": l.id.0, "blend": o.gradient_blend_mode.label()}));
    }
    if o.gradient_style == before.gradient_style && o.gradient_reverse == before.gradient_reverse && o.gradient_dither == before.gradient_dither {
        return;
    }
    let mut p = json!({"layer": l.id.0});
    if o.gradient_style != before.gradient_style {
        p["style"] = json!(o.gradient_style);
    }
    if o.gradient_reverse != before.gradient_reverse {
        p["reverse"] = json!(o.gradient_reverse);
    }
    if o.gradient_dither != before.gradient_dither {
        p["dither"] = json!(o.gradient_dither);
    }
    let _ = app.run(cmds::SET, p);
}

/// Live mode: the options bar's Opacity sets a selected gradient fill layer's opacity, as a
/// drag's `opacity` does, live while it is scrubbed; `drag` numbers the scrub so it is one undo
/// step, as in the Layers panel (#5012).
pub fn opacity_changed(app: &mut PhotocraftApp, drag: Option<u64>) {
    let o = &app.ui.tool_options;
    if app.ui.tool != Tool::Gradient || o.gradient_classic {
        return;
    }
    let opacity = o.fill_opacity / 100.0;
    let Some((l, ..)) = active_gradient(app) else { return };
    if (l.opacity - opacity).abs() > 1e-6 {
        let mut p = json!({"layer": l.id.0, "opacity": opacity});
        if let Some(n) = drag {
            p["coalesce"] = json!(format!("gradient-tool-opacity:{n}"));
        }
        let _ = app.run("layer.setProps", p);
    }
}

// ---------------------------------------------------------------- Properties panel

/// Properties panel for a live gradient. Every change uses the existing gradient commands.
pub fn properties(app: &mut PhotocraftApp, ui: &mut egui::Ui, layer: &Layer) {
    properties_panel::show(app, ui, layer);
}

fn strip_key(app: &PhotocraftApp, sink: Sink) -> egui::Id {
    // Layer IDs can repeat in different documents. Tool presets belong to the session.
    let document = match sink {
        Sink::Layer(_) => app.session.active().map(|s| s.doc.id),
        Sink::Preset => None,
    };
    sink.key().with(document)
}

/// What a stops editor instance edits: a Gradient Fill layer (`gradient.fill.stop`) or the
/// Gradient tool's current gradient (`gradient.presets.stop`, the Gradient Editor window's
/// no-layer mode). Presets keep midpoints centred, so midpoint markers are layer-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Sink {
    Layer(LayerId),
    Preset,
}

impl Sink {
    /// Uniqueness for the drag state.
    fn key(self) -> egui::Id {
        match self {
            Sink::Layer(id) => egui::Id::unique(("gradient-stops-drag", id.0)),
            Sink::Preset => egui::Id::unique(("gradient-stops-drag", "preset")),
        }
    }
    fn cmd(self) -> &'static str {
        match self {
            Sink::Layer(_) => cmds::STOP,
            Sink::Preset => gradient_presets::STOP,
        }
    }
    /// Preset gradients keep midpoints at the centre (they are not stored), so only a layer's
    /// editor shows draggable midpoint diamonds.
    fn midpoints(self) -> bool {
        matches!(self, Sink::Layer(_))
    }
    /// The edit params with the layer added (the preset grammar has no `layer`).
    fn params(self, mut p: Value) -> Value {
        if let Sink::Layer(id) = self {
            p["layer"] = json!(id.0);
        }
        p
    }
}

/// The Gradient Editor window's no-layer mode: the strip edits the Gradient tool's current
/// gradient in place (`gradient.presets.stop`), as Photoshop's editor edits the tool gradient.
fn preset_stops_editor(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if let Some(f) = preset_fill(app) {
        stops_editor_for(app, ui, Sink::Preset, f);
    }
}

/// The current gradient as a fill for the strip (foreground/background resolved now, the preset's
/// own opacity stops, empty = opaque, midpoints centred).
fn preset_fill(app: &PhotocraftApp) -> Option<Fill> {
    let (fg, bg) = (app.session.tools.foreground, app.session.tools.background);
    let (cs, os) = app.session.presets.gradient.fill_stops(fg, bg);
    Some(Fill::Gradient {
        stops: cs,
        angle: 0.0,
        scale: 1.0,
        style: GradientStyle::Linear,
        reverse: false,
        opacity_stops: os,
        midpoints: Vec::new(),
        offset: (0.0, 0.0),
        dither: false,
        align: true,
    })
}

/// Opens the Color Picker on the current gradient's colour stop `i`; OK runs
/// `gradient.presets.stop` (one journal step).
fn preset_stop_color(app: &mut PhotocraftApp, i: usize) {
    let Some(f) = preset_fill(app) else { return };
    let Fill::Gradient { stops, .. } = &f else { return };
    let mut sorted = stops.clone();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, c)) = sorted.get(i) else { return };
    let r = c.to_rgb();
    app.gradient.preset_stop = None;
    crate::color_picker_ui::open_for_command(app, "Color Picker (Stop Color)", r, gradient_presets::STOP, json!({"action": "color", "index": i}));
}

fn stops_editor_for(app: &mut PhotocraftApp, ui: &mut egui::Ui, sink: Sink, f: Fill) {
    let key = strip_key(app, sink);
    match stops_bar::show(ui, key, &f, sink.midpoints(), false) {
        Some(BarAction::Preview(p)) => match sink {
            Sink::Layer(id) => app.gradient.panel = Some((id, cmds::STOP, sink.params(p))),
            Sink::Preset => app.gradient.preset_stop = Some(p),
        },
        Some(BarAction::Commit(p)) => {
            clear_pending(app, sink);
            let _ = app.run(sink.cmd(), sink.params(p));
        }
        Some(BarAction::Clear) => clear_pending(app, sink),
        Some(BarAction::Color(i)) => match sink {
            Sink::Layer(id) => edit_stop_color(app, id, i),
            Sink::Preset => preset_stop_color(app, i),
        },
        None => {}
    }
}

fn clear_pending(app: &mut PhotocraftApp, sink: Sink) {
    match sink {
        Sink::Layer(_) => app.gradient.panel = None,
        Sink::Preset => app.gradient.preset_stop = None,
    }
}

#[cfg(test)]
mod tests;
