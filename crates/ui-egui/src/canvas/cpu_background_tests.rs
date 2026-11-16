//! The CPU canvas's frame time on the offscreen software adapter (a manual benchmark).
use super::*;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, PhotocraftApp>;

/// The CPU-canvas fixture on the offscreen software adapter, or `None` when this machine has no
/// graphics adapter. Only creating the renderer may be skipped: a failing fixture command or a
/// panic in the first frames must fail the test, not pass it as "no adapter".
fn harness(size: [u32; 2], background: &str) -> Option<Harness> {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
    // app.gpu remains None: compose on the CPU, use the harness's software window adapter.
    app.run("file.new", json!({"width": size[0], "height": size[1], "background": background})).unwrap();
    app.run("select.rect", json!({"x": size[0] / 2, "y": size[1] / 2, "width": size[0] / 10, "height": size[1] / 10})).unwrap();
    app.run("edit.fill", json!({"color": "#2050d0"})).unwrap();
    app.run("select.deselect", json!({})).unwrap();
    // `wgpu()` creates the render state and panics when no adapter is found.
    let builder = std::panic::catch_unwind(|| {
        egui_kittest::Harness::<PhotocraftApp>::builder().with_size(egui::vec2(1440.0, 900.0)).with_pixels_per_point(1.0).with_max_steps(64).wgpu()
    })
    .ok()?;
    let mut h = builder.build_eframe(move |cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        if let Some(rs) = &cc.wgpu_render_state {
            app.perf.gpu_info.set_adapter(&rs.adapter.get_info());
        }
        app
    });
    h.run_steps(12);
    Some(h)
}

<<<<<<< HEAD
=======
fn assert_fixture(h: &Harness, background: &str) {
    let key = cache_key(h.state().session.documents()[0].doc.id, None);
    assert!(h.state().session.prefs().transparency_and_gamut.square().is_some());
    assert_eq!(h.state().canvases[&key].texture_opaque, background == "white", "fixture opacity");
}

/// Draw the unchanged reference path by temporarily disabling only the opacity guarantee.
fn reference_frame(h: &mut Harness) {
    let flags: Vec<_> = h.state().canvases.iter().map(|(k, c)| (*k, c.texture_opaque)).collect();
    for c in h.state_mut().canvases.values_mut() {
        c.texture_opaque = false;
    }
    h.step();
    for (k, opaque) in flags {
        if let Some(c) = h.state_mut().canvases.get_mut(&k) {
            c.texture_opaque = opaque;
        }
    }
}

#[test]
#[should_panic(expected = "called `Result::unwrap()` on an `Err` value")]
fn fixture_failures_are_not_skipped_as_a_missing_adapter() {
    let _gpu = photocraft_testkit::gpu_lock();
    // An invalid fixture document must fail on any machine, with or without a graphics adapter.
    let _ = harness([0, 0], "white");
}

#[test]
fn omitting_an_occluded_checker_preserves_cpu_canvas_pixels() {
    let _gpu = photocraft_testkit::gpu_lock();
    let Some(mut h) = harness([320, 240], "white") else {
        eprintln!("skipping: no offscreen graphics adapter");
        return;
    };
    assert_fixture(&h, "white");
    eprintln!("CPU pixel comparison adapter: {}", serde_json::to_string(&h.state().perf.gpu_info).unwrap());
    for (rotation, flip, pattern) in
        [(0.0, false, false), (23.0, false, false), (0.0, true, false), (0.0, false, true), (23.0, false, true), (0.0, true, true), (23.0, true, true)]
    {
        h.state_mut().ui.views[0].rotation = rotation;
        h.state_mut().ui.view.flip_horizontal = flip;
        h.state_mut().ui.view.pattern_preview = pattern;
        reference_frame(&mut h);
        let reference = h.render().unwrap();
        h.step();
        let optimized = h.render().unwrap();
        if let Some(dir) = std::env::var_os("PHOTOCRAFT_CPU_PIXEL_OUTPUT") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            let tag = format!("rotation-{rotation}-flip-{flip}-pattern-{pattern}");
            reference.save(dir.join(format!("{tag}-reference.png"))).unwrap();
            optimized.save(dir.join(format!("{tag}-optimized.png"))).unwrap();
            let mut changed = 0;
            let mut bounds = [u32::MAX, u32::MAX, 0, 0];
            for (x, y, pixel) in reference.enumerate_pixels() {
                if pixel != optimized.get_pixel(x, y) {
                    changed += 1;
                    bounds = [bounds[0].min(x), bounds[1].min(y), bounds[2].max(x), bounds[3].max(y)];
                }
            }
            let report = json!({"adapter": h.state().perf.gpu_info, "rotation": rotation, "flip": flip,
                "pattern": pattern, "changed_pixels": changed, "difference_bounds": (changed > 0).then_some(bounds)});
            std::fs::write(dir.join(format!("{tag}.json")), serde_json::to_string_pretty(&report).unwrap()).unwrap();
        }
        assert!(reference == optimized, "rotation={rotation} flip={flip} pattern={pattern}");
    }
}

#[test]
fn pattern_preview_keeps_the_legacy_checker_for_every_copy() {
    let _gpu = photocraft_testkit::gpu_lock();
    let Some(mut h) = harness([320, 240], "white") else {
        eprintln!("skipping: no offscreen graphics adapter");
        return;
    };
    assert_fixture(&h, "white");
    let checker = h.state().checker.as_ref().unwrap().id();
    let checker_meshes = |h: &Harness| {
        h.output()
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Mesh(m) if m.texture_id == checker => Some(m.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    for (rotation, flip) in [(0.0, false), (23.0, false), (0.0, true), (23.0, true)] {
        h.state_mut().ui.view.pattern_preview = true;
        h.state_mut().ui.view.flip_horizontal = flip;
        let v = &mut h.state_mut().ui.views[0];
        v.zoom = 1.25;
        v.center = [159.25, 119.75];
        v.rotation = rotation;
        v.fit_pending = false;
        v.fill_pending = false;
        reference_frame(&mut h);
        let reference = checker_meshes(&h);
        assert!(reference.len() > 1, "the document and its visible copies need their checker");
        h.step();
        assert_eq!(checker_meshes(&h), reference, "rotation={rotation} flip={flip}: retain the legacy checker submissions");
    }
    h.state_mut().ui.view.pattern_preview = false;
    h.step();
    assert!(checker_meshes(&h).is_empty(), "ordinary opaque CPU canvases still omit the hidden checker");
}

>>>>>>> upstream/main
#[test]
#[ignore = "manual release benchmark: 24 MP CPU canvas on the offscreen software adapter"]
fn cpu_canvas_background_benchmark() {
    let _gpu = photocraft_testkit::gpu_lock();
    use std::time::Instant;
    let mut results = Vec::new();
    for background in ["white", "transparent"] {
        let mut h = harness([6000, 4000], background).expect("an offscreen graphics adapter");
        let v = &mut h.state_mut().ui.views[0];
        v.zoom = 1.0;
        v.center = [3000.0, 2000.0];
        v.fit_pending = false;
        v.fill_pending = false;
        for _ in 0..3 {
            h.step();
            h.render().unwrap();
        }
        let mut frames = Vec::new();
        for n in 0..24 {
            h.state_mut().ui.views[0].center[0] = 3000.0 + if n % 2 == 0 { -3.0 } else { 3.0 };
            let t = Instant::now();
            h.step();
            h.render().unwrap();
            frames.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        frames.sort_by(f64::total_cmp);
        results.push(json!({
            "background": background, "document_pixels": 24_000_000, "viewport": [1440, 900],
            "adapter": h.state().perf.gpu_info, "samples": frames.len(),
            "p50_ms": frames[11], "p95_ms": frames[22], "frames_ms": frames,
            "includes": "UI frame, the canvas's raster display and synchronous readback; no document recomposite",
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
