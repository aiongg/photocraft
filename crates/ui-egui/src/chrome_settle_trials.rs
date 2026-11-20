//! The window's chrome replays at rest (#15493): a frame whose only input is the pointer moving over the canvas
//! replays the dock's tab rows and column bars and the menu bar (memoized views) and runs no panel tab strip. The dock
//! groups, their grips and the title bar are containers, which run every pass by design.

use craft::ui::runtime::measure::{MemoRun, last_pass_memo_runs, last_pass_runs};
use egui::{Event, vec2};
use egui_kittest::Harness;
use serde_json::json;

use crate::PhotocraftApp;

type H = Harness<'static, Box<PhotocraftApp>>;

/// The chrome's memoized views in the last pass, by type name, and how each took part.
fn chrome_views(h: &H) -> Vec<(&'static str, MemoRun)> {
    const CHROME: [&str; 3] = ["docking::group::TabRow<", "dock_column::bar::BarProps<", "menu::bar::BarProps<"];
    last_pass_memo_runs(&h.ctx).into_iter().filter(|(name, _, _)| CHROME.iter().any(|c| name.contains(c))).map(|(name, _, run)| (name, run)).collect()
}

/// The panel tab strips the last pass ran.
fn tab_strip_runs(h: &H) -> Vec<(&'static str, u32)> {
    last_pass_runs(&h.ctx).into_iter().filter(|(ty, _)| ty.contains("TabStrip<'_, craft_ui_panels::docking::PanelKey>")).collect()
}

#[test]
fn a_hover_over_the_canvas_replays_the_dock_chrome_and_the_menu_bar() {
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).with_step_dt(1.0 / 60.0).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 400, "height": 300})).unwrap();
        PhotocraftApp::with_context(cc.egui_ctx.clone(), session, crate::Services::default())
    });
    h.run_steps(8);
    let at = h.state().last_canvas_rect.center();
    h.event(Event::PointerMoved(at));
    h.run_steps(4);
    assert!(!last_pass_runs(&h.ctx).is_empty(), "the frame runs components");
    for step in 1..=3 {
        h.event(Event::PointerMoved(at + vec2(step as f32, 0.0)));
        h.run_steps(1);
        let views = chrome_views(&h);
        for kind in ["TabRow", "dock_column::bar::BarProps", "menu::bar::BarProps"] {
            assert!(views.iter().any(|(name, _)| name.contains(kind)), "hover step {step}: the chrome shows a {kind} view: {views:?}");
        }
        let ran: Vec<_> = views.iter().filter(|(_, run)| *run != MemoRun::Replayed).collect();
        assert_eq!(ran, Vec::<&(&str, MemoRun)>::new(), "hover step {step}: the chrome's views replay their last run");
        assert_eq!(tab_strip_runs(&h), [], "hover step {step}: no panel tab strip runs");
    }
}
