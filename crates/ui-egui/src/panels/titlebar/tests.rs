use super::*;
use craft::ui::title_bar::{ModalRects, TitleBar, WINDOWS_BUTTON_WIDTH, caption_id};
use egui::{Color32, CursorIcon, Event, Id, Order, Pos2, RawInput, Rect, ResizeDirection, ViewportCommand, ViewportId, pos2, vec2};
use photocraft_engine::Session;
use serde_json::json;

use craft::ui::title_bar::Caption;

/// Windows' order, left to right.
const CAPTIONS: [Caption; 3] = [Caption::Minimize, Caption::Maximize, Caption::Close];

/// #5826: the drag overlay under a dialog doesn't depend on the custom title bar. On macOS
/// (OS traffic lights over the app's own title strip) a dialog left the window undraggable.
#[test]
fn a_dialog_keeps_the_title_strip_draggable_with_either_title_bar() {
    for custom in [false, true] {
        let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
        app.custom_titlebar = custom;
        app.run("file.new", serde_json::json!({"width": 8, "height": 8})).unwrap();
        assert!(!app.titlebar_modal_drag(true), "no dialog, no overlay");
        crate::filter_dialog::open(&mut app, "filter.blur.gaussianBlur").unwrap();
        assert!(app.titlebar_modal_drag(true), "custom title bar: {custom}");
        assert!(!app.titlebar_modal_drag(false), "chrome hidden (full screen): no title strip");
        app.ui.panels.menu_bar = false;
        assert!(!app.titlebar_modal_drag(true), "no menu bar, no title strip");
    }
}

/// A headless window `width` pt wide showing the title bar and the resize zones.
struct Win {
    ctx: egui::Context,
    app: PhotocraftApp,
    width: f32,
    time: f64,
    maximized: bool,
    /// Rects of dialogs open over the window, as `dialogs::show` records them.
    dialogs: Vec<Rect>,
}

impl Win {
    fn new(width: f32) -> Self {
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
        let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
        app.custom_titlebar = true;
        Self { ctx, app, width, time: 0.0, maximized: false, dialogs: vec![] }
    }

    fn frame(&mut self, events: Vec<Event>) -> egui::FullOutput {
        self.frame_with_modal(events, false)
    }

    fn frame_with_modal(&mut self, events: Vec<Event>, modal: bool) -> egui::FullOutput {
        self.time += 0.05;
        let mut input =
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(self.width, 600.0))), time: Some(self.time), events, ..Default::default() };
        if let Some(v) = input.viewports.get_mut(&ViewportId::ROOT) {
            v.maximized = Some(self.maximized);
        }
        // What `PhotocraftApp::raw_input_hook` does.
        release_after_os_resize(&self.ctx, &mut input);
        let app = &mut self.app;
        let dialogs = self.dialogs.clone();
        let mut out = self.ctx.run_ui(input, |ui| {
            if app.ui.panels.menu_bar {
                crate::panels::title_bar(app, ui);
            } else {
                edge_zones_without_bar(app, ui);
            }
            if modal {
                egui::Modal::new(Id::unique("titlebar-drag-test-modal")).backdrop_color(Color32::TRANSPARENT).show(ui.ctx(), |ui| {
                    ui.label("Modal dialog");
                });
                for &rect in &dialogs {
                    ModalRects::push(ui.ctx(), ui.ctx().viewport_id(), rect);
                }
                let rects = ModalRects::take(ui.ctx(), ui.ctx().viewport_id());
                TitleBar::modal_drag_overlay(ui.ctx(), ui.ctx().viewport_id(), &rects);
            }
        });
        out.textures_delta.clear();
        out
    }

    /// Frames for a pointer gesture: move to `path[0]`, press, move along `path`, release. The
    /// viewport commands the frames sent.
    fn gesture(&mut self, path: &[Pos2]) -> Vec<ViewportCommand> {
        self.gesture_with_modal(path, false)
    }

    fn gesture_with_modal(&mut self, path: &[Pos2], modal: bool) -> Vec<ViewportCommand> {
        self.gesture_by(path, modal, egui::PointerButton::Primary)
    }

    fn gesture_by(&mut self, path: &[Pos2], modal: bool, btn: egui::PointerButton) -> Vec<ViewportCommand> {
        let mut events = vec![vec![Event::PointerMoved(path[0])], vec![button(path[0], true, btn)]];
        events.extend(path[1..].iter().map(|p| vec![Event::PointerMoved(*p)]));
        events.push(vec![button(*path.last().unwrap_or(&path[0]), false, btn)]);
        events
            .into_iter()
            .flat_map(|e| self.frame_with_modal(e, modal).viewport_output.remove(&ViewportId::ROOT).map(|v| v.commands).unwrap_or_default())
            .collect()
    }

    fn click(&mut self, p: Pos2) -> Vec<ViewportCommand> {
        self.gesture(&[p])
    }

    fn caption(&self, c: Caption) -> Rect {
        self.ctx.read_response(caption_id(ViewportId::ROOT, c)).map(|r| r.rect).unwrap_or(Rect::NOTHING)
    }

    /// The bar's clickable widgets other than its empty area (as wide as the window) and the
    /// caption buttons.
    fn bar_widgets(&self) -> Vec<Rect> {
        let skip = CAPTIONS.map(|c| caption_id(ViewportId::ROOT, c));
        self.ctx.viewport(|v| {
            v.prev_pass
                .widgets
                .layers()
                .filter(|(l, _)| l.order != Order::Foreground)
                .flat_map(|(_, ws)| ws.iter())
                .filter(|w| w.sense.senses_click() && !w.sense.senses_drag() && w.rect.top() < 30.0 && w.rect.width() < self.width && !skip.contains(&w.id))
                .map(|w| w.rect)
                .collect()
        })
    }
}

fn texts(out: &egui::FullOutput) -> Vec<(String, Rect)> {
    fn walk(s: &egui::Shape, v: &mut Vec<(String, Rect)>) {
        match s {
            egui::Shape::Text(t) => v.push((t.galley.text().to_string(), Rect::from_min_size(t.pos, t.galley.size()))),
            egui::Shape::Vec(s) => s.iter().for_each(|s| walk(s, v)),
            _ => {}
        }
    }
    let mut v = vec![];
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut v));
    v
}

fn button(p: Pos2, pressed: bool, button: egui::PointerButton) -> Event {
    Event::PointerButton { pos: p, button, pressed, modifiers: Default::default() }
}

#[test]
fn caption_buttons_take_the_right_end_and_nothing_overlaps_them() {
    for width in [760.0, 1100.0, 1440.0] {
        let mut w = Win::new(width);
        w.frame(vec![]);
        w.frame(vec![]);
        let [min, max, close] = CAPTIONS.map(|c| w.caption(c));
        assert_eq!((close.right(), close.top()), (width, 0.0), "Close sits in the window's corner at {width}");
        // The bar's bottom point is the panel's separator line, which the buttons leave visible.
        assert!(close.height() >= 31.0 && [min, max, close].iter().all(|r| r.width() == WINDOWS_BUTTON_WIDTH));
        assert_eq!((min.right(), max.right()), (max.left(), close.left()));
        let widgets = w.bar_widgets();
        assert!(widgets.len() >= 10, "menus, search, theme, workspace: {widgets:?}");
        for a in &widgets {
            assert!(a.right() <= min.left(), "{a:?} runs into the caption buttons at {width}");
        }
    }
}

#[test]
fn the_title_bar_promotes_no_community_chat() {
    // Issue #1726: only the always-visible title-bar button is removed. The start-screen
    // and About buttons and Help › Discord remain available.
    for width in [760.0, 1100.0, 1440.0] {
        let mut w = Win::new(width);
        w.frame(vec![]);
        let out = w.frame(vec![]);
        assert!(!texts(&out).iter().any(|(t, _)| t.contains("Discord")), "no Discord in the bar at {width}");
    }
}

#[test]
fn the_brand_mark_leads_the_bar() {
    let mut w = Win::new(1440.0);
    w.frame(vec![]);
    let out = w.frame(vec![]);
    let mark = crate::brand::mark_rect(&w.ctx).expect("the brand mark is painted");
    let file = texts(&out).into_iter().find(|(t, _)| t == "File").map(|(_, r)| r).expect("the File menu title");
    assert!(mark.right() < file.left(), "the mark sits left of the menus: {mark:?} {file:?}");
    assert!(mark.left() < 20.0 && mark.width() >= 16.0, "{mark:?}");
}

#[test]
fn close_runs_file_exit_which_asks_about_unsaved_work() {
    let mut w = Win::new(1100.0);
    w.frame(vec![]);
    let close = w.caption(Caption::Close).center();
    let cmds = w.click(close);
    // File › Exit asks the window to close; the unsaved-changes guard (`discard_ui`) then
    // decides, so the caption button and the menu item behave the same.
    assert!(cmds.iter().any(|c| matches!(c, ViewportCommand::Close)), "{cmds:?}");
}

/// After the window's GPU device is lost, the prompt asking about unsaved work could never be
/// drawn: the close button quits instead of opening it (see `discard_ui::intercept`).
#[test]
fn close_quits_without_a_prompt_once_the_window_device_is_lost() {
    let mut w = Win::new(1100.0);
    w.app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    w.app.run("layer.new.layer", json!({})).unwrap();
    w.frame(vec![]);
    let close = w.caption(Caption::Close).center();
    let asks = w.click(close);
    assert!(!asks.iter().any(|c| matches!(c, ViewportCommand::Close)), "a window that can draw asks first: {asks:?}");
    assert!(w.app.discard.is_some());
    w.app.discard = None;
    w.app.window_lost = true;
    let quits = w.click(close);
    assert!(quits.iter().any(|c| matches!(c, ViewportCommand::Close)), "{quits:?}");
    assert!(w.app.discard.is_none(), "no prompt opened");
}

#[test]
fn empty_bar_space_still_drags_with_a_modal_open() {
    let mut w = Win::new(1440.0);
    w.frame(vec![]);
    w.frame_with_modal(vec![], true);
    let empty = pos2(980.0, 16.0);
    assert!(w.bar_widgets().iter().all(|r| !r.contains(empty)), "the probe point must be empty bar space");
    let cmds = w.gesture_with_modal(&[empty, empty + vec2(12.0, 4.0), empty + vec2(30.0, 8.0)], true);
    assert_eq!(cmds.iter().filter(|c| matches!(c, ViewportCommand::StartDrag)).count(), 1, "{cmds:?}");
}

#[test]
fn a_dialog_in_the_gap_keeps_the_dialog_the_drag() {
    let mut w = Win::new(1440.0);
    w.frame(vec![]);
    w.frame_with_modal(vec![], true);
    let area = Rect::from_center_size(pos2(980.0, 16.0), vec2(200.0, 30.0));
    w.dialogs = vec![area];
    let cmds = w.gesture_with_modal(&[area.center(), area.center() + vec2(12.0, 4.0)], true);
    assert!(!cmds.iter().any(|c| matches!(c, ViewportCommand::StartDrag)), "{cmds:?}");
}

#[test]
fn a_hidden_menu_bar_keeps_the_edges_resizing() {
    let mut w = Win::new(1000.0);
    w.app.ui.panels.menu_bar = false;
    w.frame(vec![]);
    let p = pos2(1.0, 300.0);
    let out = w.frame(vec![Event::PointerMoved(p)]);
    assert_eq!(out.platform_output.cursor_icon, CursorIcon::ResizeWest);
    assert_eq!(w.gesture(&[p, p + vec2(6.0, 0.0)]), vec![ViewportCommand::BeginResize(ResizeDirection::West)]);
}

#[test]
fn gtk_layouts_give_the_caption_style() {
    use craft::window::Button::{Close, Maximize, Minimize};
    let s = CaptionStyle::from_gtk_layout(CaptionFamily::Adwaita, "appmenu:minimize,maximize,close");
    assert_eq!((s.captions.layout.left, s.captions.layout.right), (vec![], vec![Minimize, Maximize, Close]));
    let s = CaptionStyle::from_gtk_layout(CaptionFamily::Breeze, "minimize,close");
    assert_eq!((s.captions.family, s.captions.layout.left), (CaptionFamily::Breeze, vec![Minimize, Close]), "no colon: GTK puts them all at the start");
    assert_eq!(CaptionStyle::default().captions, Captions { family: CaptionFamily::Windows, layout: craft::window::Layout::default() });
}

#[test]
fn linux_buttons_sit_where_the_layout_says_and_drag_the_window() {
    for (family, size, gap, end) in [(CaptionFamily::Adwaita, 24.0, 6.5, 10.0), (CaptionFamily::Breeze, 18.0, 4.5, 8.0)] {
        let width = 1100.0;
        let mut w = Win::new(width);
        let layout =
            craft::window::Layout { left: vec![craft::window::Button::Close], right: vec![craft::window::Button::Minimize, craft::window::Button::Maximize] };
        w.app.services.caption_style = Some(Box::new(move |_| CaptionStyle { captions: Captions { family, layout: layout.clone() }, icons: None }));
        w.frame(vec![]);
        w.frame(vec![]);
        let [min, max, close] = CAPTIONS.map(|c| w.caption(c));
        // Flush with the window's edges, the edge gap outermost, the hit areas touching.
        assert_eq!((close.left(), close.width()), (0.0, end + size + gap), "{family:?}");
        assert_eq!((max.right(), max.width()), (width, gap + size + end), "{family:?}");
        assert_eq!((min.right(), min.width()), (max.left(), 2.0 * gap + size), "{family:?}");
        // GNOME's header bars are taller than Photoshop's bar; KDE's are not. The round buttons
        // take the whole bar, its separator included.
        if family == CaptionFamily::Adwaita {
            assert_eq!(close.height(), 47.0);
        } else {
            assert!((32.0..46.0).contains(&close.height()), "{close:?}");
        }
        let mark = crate::brand::mark_rect(&w.ctx).expect("the brand mark is painted");
        assert!(mark.left() > close.right(), "the bar starts after the left buttons: {mark:?} {close:?}");
        for a in w.bar_widgets() {
            assert!(a.left() >= close.right() && a.right() <= min.left(), "{a:?} runs into the caption buttons ({family:?})");
        }
        assert!(w.click(max.center()).contains(&ViewportCommand::Maximized(true)));
        assert!(w.click(close.center()).iter().any(|c| matches!(c, ViewportCommand::Close)));
        // A drag that starts on a button moves the window instead of pressing it.
        let at = min.center();
        let cmds = w.gesture(&[at, at + vec2(12.0, 4.0), at + vec2(30.0, 8.0)]);
        assert_eq!(cmds.iter().filter(|c| matches!(c, ViewportCommand::StartDrag)).count(), 1, "{cmds:?}");
        assert!(!cmds.contains(&ViewportCommand::Minimized(true)), "{cmds:?}");
    }
}
