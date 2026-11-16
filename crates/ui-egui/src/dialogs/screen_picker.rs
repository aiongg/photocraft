//! PhotoCraft's driver over its one framework [`ScreenPick`]: every screen colour pick, from the
//! Color Picker dialog and from colour popovers, runs through these functions. The capture, the
//! capture windows, the loupe and the pixel mapping are the framework's
//! (`craft::color::ui::{ScreenPick, ScreenColorPicker}`).
use crate::PhotocraftApp;
use craft::color::ui::{ScreenColorPicker, ScreenPickGesture, ScreenPickerWords};
use craft::theme::Srgba;
use craft::ui::runtime::env::Env;
use egui::Context;
use std::hash::{Hash, Hasher};

/// What a pick belongs to: the active document, its active layer and the top dialog. A pick whose
/// target changes before it ends is cancelled, and a colour it left is dropped.
fn target_hash(app: &PhotocraftApp) -> u64 {
    let active = app.session.active();
    let target = (active.map(|s| s.doc.id), active.and_then(|s| s.active_layer), app.framework.dialogs.stack().top());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hasher);
    hasher.finish()
}
<<<<<<< HEAD

/// Runs the pick for one pass, before the shortcuts: starts a requested capture (the test source
/// in `services.screen_capture`, else the platform capture) and settles a finished one. Returns
/// whether a pick is still running.
=======
pub enum Capture {
    Images(Vec<ScreenImage>),
    /// Native portal/browser pickers return sRGB; None means cancellation.
    Color(Option<[f32; 3]>),
}
/// Dropping a pending request also cancels portal/browser work.
pub struct Pending {
    pub receiver: Receiver<Result<Capture, String>>,
    pub cancelled: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
pub type Service = Box<dyn FnMut(&Context) -> Pending>;
pub const MAX_PIXELS: usize = 64 * 1024 * 1024;
impl ScreenImage {
    pub fn validate(&self) -> Result<(), String> {
        let count = self.width.checked_mul(self.height).filter(|&n| n > 0 && n <= MAX_PIXELS).ok_or("Screen image is too large or empty")?;
        if count.checked_mul(4) != Some(self.rgba.len())
            || !self.position.is_finite()
            || !self.size.is_finite()
            || self.size.min_elem() <= 0.0
            || self.size.max_elem() > 65535.0
            || self.position.to_vec2().abs().max_elem() > 1_000_000.0
            || self.profile.as_ref().is_some_and(|p| p.len() > 1024 * 1024)
        {
            return Err("Invalid screen image geometry".into());
        }
        Ok(())
    }
    fn pixel(&self, x: usize, y: usize) -> Option<Color32> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = y.checked_mul(self.width)?.checked_add(x)?.checked_mul(4)?;
        let p = self.rgba.get(start..start.checked_add(4)?)?;
        Some(Color32::from_rgb(*p.first()?, *p.get(1)?, *p.get(2)?))
    }
    fn at(&self, rect: Rect, pos: Pos2) -> Option<(usize, usize)> {
        if !rect.contains(pos) || !pos.is_finite() || rect.size().min_elem() <= 0.0 {
            return None;
        }
        let p = (pos - rect.min) / rect.size();
        Some((((p.x * self.width as f32) as usize).min(self.width.saturating_sub(1)), ((p.y * self.height as f32) as usize).min(self.height.saturating_sub(1))))
    }
    fn conversion(&self) -> Result<Option<Arc<craft_cms::Transform>>, String> {
        let Some(bytes) = &self.profile else { return Ok(None) };
        let source = craft_cms::Profile::parse(bytes).map_err(|e| e.to_string())?;
        if source.color_space != craft_cms::ColorSpace::Rgb {
            return Err("Screen profile must describe RGB pixels".into());
        }
        craft_cms::Transform::new(&source, craft_cms::Builtin::Srgb.profile(), craft_cms::Intent::RelativeColorimetric, true)
            .map(|t| Some(Arc::new(t)))
            .map_err(|e| e.to_string())
    }
    fn sample(&self, x: usize, y: usize, conversion: Option<&craft_cms::Transform>) -> Result<[f32; 3], String> {
        let p = self.pixel(x, y).ok_or("Screen pixel is out of bounds")?;
        let mut rgb = [f32::from(p.r()) / 255.0, f32::from(p.g()) / 255.0, f32::from(p.b()) / 255.0];
        if let Some(t) = conversion {
            t.apply(&mut rgb, 3).map_err(|e| e.to_string())?;
        }
        Ok(rgb.map(|v| v.clamp(0.0, 1.0)))
    }
}
#[derive(Clone)]
struct Snapshot {
    image: Arc<ScreenImage>,
    texture: egui::TextureHandle,
    conversion: Option<Arc<craft_cms::Transform>>,
}
type Target = (Option<photocraft_doc::DocId>, Option<photocraft_doc::LayerId>, Option<u64>);
#[derive(Default)]
struct Runtime {
    request: Option<Id>,
    owner: Option<Id>,
    pending: Option<Pending>,
    images: Vec<Snapshot>,
    answer: Option<Result<Option<[f32; 3]>, String>>,
    completed: Option<(Id, [f32; 3])>,
    zoom: usize,
    generation: u64,
    target: Option<Target>,
}
type Shared = Arc<Mutex<Runtime>>;
fn runtime(ctx: &Context) -> Shared {
    let id = Id::unique("screen-color-picker-runtime");
    ctx.data_mut(|d| {
        if let Some(r) = d.get_temp::<Shared>(id) {
            return r;
        }
        let r = Arc::new(Mutex::new(Runtime::default()));
        d.insert_temp(id, r.clone());
        r
    })
}
fn lock(r: &Shared) -> std::sync::MutexGuard<'_, Runtime> {
    r.lock().unwrap_or_else(|e| e.into_inner())
}
/// Consume a result only at its initiating widget, including when its compact popup closed.
pub fn take(ctx: &Context, id: Id) -> Option<[f32; 3]> {
    let r = runtime(ctx);
    let mut s = lock(&r);
    if s.completed.as_ref().is_some_and(|(owner, _)| *owner == id) { s.completed.take().map(|(_, rgb)| rgb) } else { None }
}
/// Same action for full dialogs and compact swatches. Cancellation returns no change.
pub fn button(ui: &mut egui::Ui, id: Id) -> Option<[f32; 3]> {
    let r = runtime(ui.ctx());
    let available = ui.ctx().data(|d| d.get_temp::<bool>(Id::unique("screen-color-picker-available"))).unwrap_or(false);
    let mut state = lock(&r);
    let result = if state.completed.as_ref().is_some_and(|(owner, _)| *owner == id) { state.completed.take().map(|(_, rgb)| rgb) } else { None };
    let busy = state.owner.is_some() || state.request.is_some();
    if ui.add_enabled(available && !busy, egui::Button::new(tl!("Pick screen color"))).on_hover_text(tl!("Sample a pixel anywhere on the screen")).clicked() {
        state.request = Some(id);
        ui.ctx().request_repaint();
    }
    result
}
fn target(app: &PhotocraftApp) -> Target {
    (app.session.active().map(|s| s.doc.id), app.session.active().and_then(|s| s.active_layer), app.ui.dialogs.last().map(|d| d.id))
}
/// Poll before shortcuts. A changed editing target invalidates a pending pick.
>>>>>>> upstream/main
pub fn tick(app: &mut PhotocraftApp, ctx: &Context) -> bool {
    let target = target_hash(app);
    match &app.services.screen_capture {
        Some(make) => app.screen_pick.tick_with(ctx, target, &|| make()),
        None => app.screen_pick.tick(ctx, target),
    }
    busy(app)
}

/// Follows the picker button's press for `owner` (a dialog id, a popover id): a click starts a
/// capture; a held drag inside the app's windows returns the live sample under the pointer.
pub fn hold(app: &mut PhotocraftApp, ctx: &Context, owner: impl Hash, gesture: ScreenPickGesture) -> Option<Srgba> {
    app.screen_pick.hold(ctx, owner, gesture)
}

<<<<<<< HEAD
/// The colour `owner`'s pick ended with, once; another owner gets nothing.
pub fn take(app: &mut PhotocraftApp, owner: impl Hash) -> Option<Srgba> {
    app.screen_pick.take(owner)
}

/// Whether a pick runs or waits for its capture; the app takes no other input meanwhile.
pub fn busy(app: &PhotocraftApp) -> bool {
    app.screen_pick.busy()
}
=======
/// What the picker resolves to: the colour from a pick, or `Ok(None)` to cancel.
type Answer = Result<Option<[f32; 3]>, String>;

/// One frame of a picker window over its snapshot `image`. The picker covers the whole screen
/// and stays on top, so every way out must work (#4291): Esc, closing the window, a right-click,
/// and a left click (or a press and release that moved) anywhere, which picks the pixel under
/// it, or cancels when there's none to sample. Nothing it gets leaves the user stuck in it.
fn frame(ui: &mut egui::Ui, shared: &Shared, image: &ScreenImage, texture: &egui::TextureHandle, conversion: Option<&craft_cms::Transform>) {
    let ctx = ui.ctx().clone();
    let answer = |a: Answer| {
        lock(shared).answer = Some(a);
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    };
    if ctx.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape) || i.pointer.secondary_released()) {
        answer(Ok(None));
        return;
    }
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
        let rect = ui.max_rect();
        ui.painter().image(texture.id(), rect, Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        let response = ui.interact(rect, ui.scope_id().with("screen-sample"), Sense::click_and_drag());
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        let released = response.clicked() || response.drag_stopped();
        let pixel = ui.ctx().pointer_latest_pos().and_then(|pos| Some((pos, image.at(rect, pos)?)));
        let Some((pos, (x, y))) = pixel else {
            // No pixel under the pointer to sample: a click still ends the picker.
            if released {
                answer(Ok(None));
            }
            return;
        };
        let zoom = {
            let delta = ui.input(|i| {
                i.raw
                    .events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::MouseWheel { delta, .. } => Some(delta.y),
                        _ => None,
                    })
                    .sum::<f32>()
            });
            let mut s = lock(shared);
            if delta > 0.0 {
                s.zoom = (s.zoom + 2).min(24);
            }
            if delta < 0.0 {
                s.zoom = s.zoom.saturating_sub(2).max(4);
            }
            s.zoom
        };
        let picked = image.sample(x, y, conversion);
        loupe(ui, image, rect, pos, (x, y), zoom, picked.as_ref().ok().copied());
        if released {
            answer(picked.map(Some));
        }
    });
}
fn loupe(ui: &egui::Ui, image: &ScreenImage, screen: Rect, pos: Pos2, pixel: (usize, usize), zoom: usize, rgb: Option<[f32; 3]>) {
    let (x, y) = pixel;
    let t = crate::theme::Tokens::get(ui.ctx());
    // The label describes physical pixel magnification, including Retina and UI zoom.
    let cell = zoom as f32 / ui.ctx().pixels_per_point();
    let p = ui.painter();
    let pixel = image.pixel(x, y).unwrap_or(Color32::BLACK);
    let color = rgb.map(crate::color_picker_ui::hex).unwrap_or_else(|| format!("#{:02x}{:02x}{:02x}", pixel.r(), pixel.g(), pixel.b()));
    let label = p.layout_no_wrap(format!("{}   {}×", color.to_uppercase(), zoom), egui::FontId::monospace(12.0), t.text);
    let hint = p.layout_no_wrap(tl!("Click to pick · Esc to cancel").to_owned(), egui::FontId::proportional(11.0), t.text);
    let wheel = p.layout_no_wrap(tl!("Scroll to zoom").to_owned(), egui::FontId::proportional(11.0), t.text);
    let width = (11.0 * cell + 12.0).max(label.size().x + 16.0).max(hint.size().x + 16.0).max(wheel.size().x + 16.0);
    let size = vec2(width, 11.0 * cell + 76.0);
    let desired = pos + vec2(24.0, 24.0);
    let at = egui::pos2(desired.x.min(screen.right() - size.x).max(screen.left()), desired.y.min(screen.bottom() - size.y).max(screen.top()));
    let area = Rect::from_min_size(at, size);
    p.rect_filled(area, t.radius, t.card);
    p.rect_stroke(area, t.radius, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
    let origin = at + vec2((width - 11.0 * cell) / 2.0, 6.0);
    for row in 0..11usize {
        for col in 0..11usize {
            let px = x.checked_add(col).and_then(|v| v.checked_sub(5));
            let py = y.checked_add(row).and_then(|v| v.checked_sub(5));
            let color = px.zip(py).and_then(|(x, y)| image.pixel(x, y)).unwrap_or(t.canvas);
            p.rect_filled(Rect::from_min_size(origin + vec2(col as f32 * cell, row as f32 * cell), vec2(cell, cell)), 0.0, color);
        }
    }
    let center = Rect::from_min_size(origin + vec2(5.0 * cell, 5.0 * cell), vec2(cell, cell));
    p.rect_stroke(center.expand(1.0), 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Inside);
    p.rect_stroke(center, 0.0, Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
    let footer = at + vec2(8.0, 11.0 * cell + 14.0);
    p.galley(footer, label, t.text);
    p.galley(footer + vec2(0.0, 20.0), hint, t.text);
    p.galley(footer + vec2(0.0, 36.0), wheel, t.text);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> ScreenImage {
        ScreenImage {
            monitor: None,
            position: egui::pos2(-100.0, 0.0),
            size: vec2(2.0, 1.0),
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 128, 255, 255],
            profile: None,
        }
    }
    #[test]
    fn screen_picker_viewport_starts_hidden_until_its_snapshot_is_painted() {
        let builder = viewport_builder(&Context::default(), &image());
        assert_eq!(builder.visible, Some(false));
    }
    #[test]
    fn screen_picker_viewport_is_not_created_always_on_top() {
        // Creating the monitor-sized window topmost fails DX12 swapchain creation (Windows).
        let builder = viewport_builder(&Context::default(), &image());
        assert_eq!(builder.window_level, None);
    }
    #[test]
    fn pixel_mapping_handles_display_edges_negative_origins_and_hidpi() {
        let i = image();
        let rect = Rect::from_min_size(i.position, i.size);
        assert_eq!(i.at(rect, egui::pos2(-100.0, 0.0)), Some((0, 0)));
        assert_eq!(i.at(rect, egui::pos2(-98.0, 1.0)), Some((1, 0)));
        assert_eq!(i.at(rect, egui::pos2(-97.0, 0.0)), None);
        assert_eq!(i.at(rect, egui::pos2(f32::NAN, 0.0)), None);
        let scaled = Rect::from_min_size(Pos2::ZERO, vec2(4.0, 2.0));
        assert_eq!(i.at(scaled, egui::pos2(2.5, 1.0)), Some((1, 0)));
        assert_eq!(i.pixel(2, 0), None);
        assert_eq!(i.sample(1, 0, i.conversion().unwrap().as_deref()).unwrap(), [0.0, 128.0 / 255.0, 1.0]);
    }
    #[test]
    fn malformed_images_and_non_rgb_profiles_fail_before_sampling() {
        let mut i = image();
        assert!(i.validate().is_ok());
        i.rgba.pop();
        assert!(i.validate().is_err());
        i.width = usize::MAX;
        i.height = 2;
        assert!(i.validate().is_err());
        let mut i = image();
        i.size.x = f32::NAN;
        assert!(i.validate().is_err());
        let mut i = image();
        i.profile = Some(craft_cms::Builtin::LabD50.profile().to_bytes().as_ref().clone());
        assert!(i.conversion().is_err());
    }
    /// The picker window's frame over [`image`] (red | azure) filling a 200 × 100 window, with its
    /// own runtime: what the frame answered after `act`.
    fn picker_answer(act: impl FnOnce(&mut egui_kittest::Harness<'_>)) -> Option<Answer> {
        let shared: Shared = Arc::default();
        let inner = shared.clone();
        let texture: std::cell::OnceCell<egui::TextureHandle> = std::cell::OnceCell::new();
        let image = image();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(200.0, 100.0)).build_ui(move |ui| {
            let tex = texture.get_or_init(|| {
                ui.ctx().load_texture("test-snapshot", egui::ColorImage::new([2, 1], vec![Color32::RED, Color32::BLUE]), TextureOptions::NEAREST)
            });
            frame(ui, &inner, &image, tex, None);
        });
        h.run();
        act(&mut h);
        h.run();
        lock(&shared).answer.take()
    }
    /// #4291: the picker covers the screen and stays on top, so every input ends it: Esc, a
    /// right-click, a click (the pixel under it), a press dragged before its release (the pixel
    /// where it's released).
    #[test]
    fn every_way_out_of_the_picker_ends_it() {
        let left = egui::pos2(40.0, 50.0);
        let right = egui::pos2(160.0, 50.0);
        assert_eq!(picker_answer(|h| h.hover_at(left)), None, "hovering alone keeps it open");
        assert_eq!(
            picker_answer(|h| {
                h.hover_at(left);
                h.run();
                h.drag_at(left);
                h.run();
                h.drop_at(left);
            }),
            Some(Ok(Some([1.0, 0.0, 0.0]))),
            "a click picks the pixel under it"
        );
        assert_eq!(
            picker_answer(|h| {
                h.drag_at(left);
                h.run();
                h.hover_at(right);
                h.run();
                h.drop_at(right);
            }),
            Some(Ok(Some([0.0, 128.0 / 255.0, 1.0]))),
            "a press dragged across picks where it's released, instead of being ignored"
        );
        assert_eq!(
            picker_answer(|h| {
                h.hover_at(left);
                h.run();
                h.event(egui::Event::PointerButton { pos: left, button: egui::PointerButton::Secondary, pressed: true, modifiers: Default::default() });
                h.run();
                h.event(egui::Event::PointerButton { pos: left, button: egui::PointerButton::Secondary, pressed: false, modifiers: Default::default() });
            }),
            Some(Ok(None)),
            "a right-click cancels"
        );
        assert_eq!(picker_answer(|h| h.key_press(egui::Key::Escape)), Some(Ok(None)), "Esc cancels");
    }
    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", serde_json::json!({"width":8,"height":8})).unwrap();
        app
    }
    fn queued(app: &mut PhotocraftApp, ctx: &Context) -> (Id, std::sync::mpsc::Sender<Result<Capture, String>>, Arc<std::sync::atomic::AtomicBool>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = cancelled.clone();
        let mut receiver = Some(rx);
        app.services.screen_pick = Some(Box::new(move |_| Pending { receiver: receiver.take().unwrap(), cancelled: stop.clone() }));
        let owner = Id::unique("test-destination");
        lock(&runtime(ctx)).request = Some(owner);
        assert!(tick(app, ctx));
        (owner, tx, cancelled)
    }
    #[test]
    fn cancel_while_queued_does_not_start_or_poison_the_next_pick() {
        let ctx = Context::default();
        let mut app = app();
        let launched = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = launched.clone();
        app.services.screen_pick = Some(Box::new(move |_| {
            started.store(true, std::sync::atomic::Ordering::Relaxed);
            let (_tx, rx) = std::sync::mpsc::channel();
            Pending { receiver: rx, cancelled: Arc::new(std::sync::atomic::AtomicBool::new(false)) }
        }));
        let owner = Id::unique("queued");
        lock(&runtime(&ctx)).request = Some(owner);
        cancel(&ctx);
        assert!(!tick(&mut app, &ctx));
        assert!(!launched.load(std::sync::atomic::Ordering::Relaxed));
        assert_eq!(take(&ctx, owner), None);
        // An idle Cancel must not be retained as the reply to a future request.
        cancel(&ctx);
        let (owner, tx, _) = queued(&mut app, &ctx);
        tx.send(Ok(Capture::Color(Some([0.2, 0.4, 0.6])))).unwrap();
        assert!(!tick(&mut app, &ctx));
        assert_eq!(take(&ctx, owner), Some([0.2, 0.4, 0.6]));
    }
>>>>>>> upstream/main

/// Ends a running or requested pick without a colour.
pub fn cancel(app: &mut PhotocraftApp) {
    app.screen_pick.cancel();
}

/// Shows the running pick's capture windows, once per pass after the app's UI, and puts a capture
/// failure in the status bar.
pub fn show(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    // A frame sampled during a held drag converts from the monitor's profile to sRGB. The engine's profile is its own
    // type, so the picker gets its ICC bytes read as the framework's; only a running pick needs it.
    if app.screen_pick.busy()
        && let Ok(monitor) = craft::cms::Profile::parse(&app.session.color.monitor().to_bytes())
    {
        app.screen_pick.set_monitor(&monitor);
    }
    ScreenColorPicker::new(&mut app.screen_pick, "PhotoCraft").show(ui);
    // The status is taken here rather than in `tick`: the words come from the Env, which needs a
    // `Ui`, and `show` runs after every settle of the pass (tick's and the windows').
    let words = Env::of(ui).words::<ScreenPickerWords>();
    if let Some(message) = app.screen_pick.take_status(&words) {
        app.ui.status = message.text.as_str().to_owned();
        app.ui.status_error = true;
    }
}
