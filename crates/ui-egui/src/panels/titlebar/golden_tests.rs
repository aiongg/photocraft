//! Golden renders of the title bar in every caption family, at rest, with a button under the
//! pointer and maximized, in a dark and a light theme. `UPDATE_SNAPSHOTS=1` rewrites them
//! (`crates/ui-egui/tests/snapshots/title-bar/`).

use super::*;
use crate::theme::ThemeKind;
use egui_kittest::kittest::Queryable;
use egui_kittest::{Harness, SnapshotOptions};
use photocraft_engine::Session;

const WIDTH: f32 = 760.0;
/// The tallest bar (GNOME's 47 pt) and the panel below it.
const HEIGHT: f32 = 56.0;

#[derive(Clone, Copy)]
enum State {
    Rest,
    HoverMinimize,
    HoverClose,
    Maximized,
}

impl State {
    const ALL: [State; 4] = [State::Rest, State::HoverMinimize, State::HoverClose, State::Maximized];

    fn name(self) -> &'static str {
        match self {
            State::Rest => "rest",
            State::HoverMinimize => "hover-minimize",
            State::HoverClose => "hover-close",
            State::Maximized => "maximized",
        }
    }
}

/// A distinct mask per glyph, so the icon-theme path shows in the render.
fn test_icon(icon: CaptionIcon, px: u32) -> Option<Vec<u8>> {
    let n = px as usize;
    let mask = (0..n * n)
        .map(|i| {
            let (x, y) = (i % n, i / n);
            let on = match icon {
                CaptionIcon::Minimize => y >= n * 3 / 4,
                CaptionIcon::Maximize => x == 0 || y == 0 || x + 1 == n || y + 1 == n,
                CaptionIcon::Restore => (n / 4..n * 3 / 4).contains(&x) && (n / 4..n * 3 / 4).contains(&y),
                CaptionIcon::Close => x == y || x + y + 1 == n,
            };
            if on { 255 } else { 0 }
        })
        .collect();
    Some(mask)
}

/// The families PhotoCraft draws, each with a layout that puts its buttons where that desktop
/// does (Generic's on the left, to cover that end).
fn styles() -> Vec<(&'static str, CaptionStyle)> {
    let icons = CaptionIcons { theme: "Test".into(), render: std::sync::Arc::new(test_icon) };
    let mut adwaita_icons = CaptionStyle::from_gtk_layout(CaptionFamily::Adwaita, "appmenu:minimize,maximize,close");
    adwaita_icons.icons = Some(icons);
    vec![
        ("windows", CaptionStyle::default()),
        ("adwaita", CaptionStyle::from_gtk_layout(CaptionFamily::Adwaita, "appmenu:minimize,maximize,close")),
        ("adwaita-icons", adwaita_icons),
        ("breeze", CaptionStyle::from_gtk_layout(CaptionFamily::Breeze, ":minimize,maximize,close")),
        ("generic", CaptionStyle::from_gtk_layout(CaptionFamily::Generic, "close,maximize,minimize:")),
    ]
}

fn render(kind: ThemeKind, style: CaptionStyle, state: State) -> Harness<'static, PhotocraftApp> {
    let mut app = PhotocraftApp::new(Session::new(), crate::Services::default());
    app.custom_titlebar = true;
    app.services.caption_style = Some(Box::new(move |_| style.clone()));
    let mut ready = false;
    let mut h = Harness::builder().with_size(vec2(WIDTH, HEIGHT)).with_pixels_per_point(1.0).with_step_dt(1.0 / 60.0).with_max_steps(64).wgpu().build_ui_state(
        move |ui, app: &mut PhotocraftApp| {
            // Fonts and theme take effect from the next frame.
            if !ready {
                PhotocraftApp::setup_context(ui.ctx(), kind);
                ready = true;
                return;
            }
            // At the window's edges, as in the app: kittest's root frame has an 8 pt margin.
            let screen = ui.ctx().content_rect();
            let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(screen));
            ui.set_clip_rect(screen);
            crate::panels::title_bar(app, ui);
        },
        app,
    );
    h.input_mut().viewports.entry(egui::ViewportId::ROOT).or_default().maximized = Some(matches!(state, State::Maximized));
    h.run_steps(4);
    let hover = match state {
        State::HoverMinimize => Some("Minimize"),
        State::HoverClose => Some("Close"),
        State::Rest | State::Maximized => None,
    };
    if let Some(label) = hover {
        // A fixed height in the bar: the buttons' hit rects differ in height between families.
        let rect = h.get_by_label(label).rect();
        let at = egui::pos2(rect.center().x, rect.top() + 12.0);
        h.event(egui::Event::PointerMoved(at));
    }
    // Past libadwaita's 0.2 s disc animation, short of the 0.5 s tooltip delay.
    h.run_steps(20);
    h
}

#[test]
fn the_title_bar_renders_as_recorded_in_every_caption_family() {
    let _gpu = photocraft_testkit::gpu_lock();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/title-bar");
    let options = SnapshotOptions::new().threshold(0.0).output_path(dir);
    let mut failures = vec![];
    for kind in [ThemeKind::ProMedium, ThemeKind::ProLight] {
        for (family, style) in styles() {
            for state in State::ALL {
                let name = format!("{family}-{}-{}", kind.id(), state.name());
                if let Err(e) = render(kind, style.clone(), state).try_snapshot_options(&name, &options) {
                    failures.push(format!("{name}: {e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
