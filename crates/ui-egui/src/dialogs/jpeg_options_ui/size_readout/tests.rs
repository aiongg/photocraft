use super::*;
use egui_kittest::kittest::Queryable;
use serde_json::json;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// A 96 × 64 RGB document of 2 × 2 noise and gradients: detail a JPEG's quality shows in its size.
fn detailed() -> Arc<Document> {
    let mut doc = Document::with_background(
        "x",
        photocraft_doc::Size::new(96, 64),
        photocraft_doc::ColorMode::Rgb,
        photocraft_doc::SampleType::U8,
        photocraft_doc::Color::WHITE,
    );
    let id = doc.layers[0].id;
    let surf = doc.layer_mut(id).unwrap().surface_mut().unwrap();
    let mut seed = 12345u32;
    for y in (0..64).step_by(2) {
        for x in (0..96).step_by(2) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = (seed >> 24) as f32 / 255.0;
            surf.fill_rect(photocraft_geom::Rect::new(x, y, x + 2, y + 2), &[v, x as f32 / 96.0, y as f32 / 64.0, 1.0]);
        }
    }
    Arc::new(doc)
}

fn source(own_quality: Option<u8>) -> Source {
    Source { doc: detailed(), revision: 1, base: ExportSettings::default(), own_quality }
}

/// The readout is the size of the file the save writes: Maximum (12) is larger than Low (0), and
/// each matches the bytes written with the settings OK would use.
#[test]
fn quality_12_reads_larger_than_quality_0() {
    let src = source(None);
    let (low, high) = (src.settings(0, JpegFormat::Baseline), src.settings(12, JpegFormat::Baseline));
    let size = |s: &ExportSettings| photocraft_io::export(&src.doc, "a.jpg", &s.io_options()).unwrap().bytes.len() as u64;
    assert_eq!(encoded_size(&src.doc, &low), Some(size(&low)));
    assert_eq!(encoded_size(&src.doc, &high), Some(size(&high)));
    assert!(size(&high) > size(&low), "quality 12: {} bytes, quality 0: {} bytes", size(&high), size(&low));
    assert!(size(&high) > 2 * size(&low), "the quality is what is encoded, not a default");
}

/// Left on an opened JPEG's own quality, OK writes that quality exactly, so the readout encodes
/// it too; another step encodes the step's quality.
#[test]
fn the_files_own_quality_is_what_is_estimated() {
    let src = source(Some(95));
    assert_eq!(src.settings(11, JpegFormat::Progressive).jpeg_quality, Some(95));
    assert_eq!(src.settings(11, JpegFormat::Progressive).jpeg_format, JpegFormat::Progressive);
    assert_eq!(src.settings(10, JpegFormat::Baseline).jpeg_quality, Some(photocraft_engine::file_cmds::jpeg_quality(10.0)));
}

/// Waits for `readout` to answer `key`, encoding with `job`.
fn answer(readout: &mut Readout, key: Key, job: impl Fn() -> Option<u64> + Send + Sync + Clone + 'static) -> Option<u64> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(size) = readout.update(key, || job.clone(), || {}) {
            return size;
        }
        assert!(Instant::now() < deadline, "no size arrived for {key:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A pass that changes nothing encodes nothing: the size is kept per settings. A change encodes
/// once, answering `None` until the new size is ready, and only one encode runs at a time.
#[test]
fn unchanged_settings_are_not_encoded_again() {
    let encodes = Arc::new(AtomicU32::new(0));
    let counted = |size: u64| {
        let encodes = encodes.clone();
        move || {
            encodes.fetch_add(1, Ordering::SeqCst);
            Some(size)
        }
    };
    let key = Key { doc: photocraft_doc::DocId(1), revision: 3, quality: 84, format: JpegFormat::Baseline };
    let mut readout = Readout::default();
    assert_eq!(answer(&mut readout, key, counted(1000)), Some(1000));
    for _ in 0..10 {
        assert_eq!(readout.update(key, || counted(1), || {}), Some(Some(1000)));
    }
    assert_eq!(encodes.load(Ordering::SeqCst), 1, "the same settings are encoded once");

    let progressive = Key { format: JpegFormat::Progressive, ..key };
    let (gate, wait) = std::sync::mpsc::channel::<()>();
    let wait = Arc::new(Mutex::new(wait));
    let slow = {
        let encodes = encodes.clone();
        move || {
            let _ = wait.lock().unwrap().recv();
            encodes.fetch_add(1, Ordering::SeqCst);
            Some(2000)
        }
    };
    assert_eq!(readout.update(progressive, || slow.clone(), || {}), None, "no size while the encode runs");
    let other = Key { quality: 92, ..key };
    assert_eq!(readout.update(other, || counted(3), || {}), None, "a second encode waits for the first");
    gate.send(()).unwrap();
    assert_eq!(answer(&mut readout, progressive, slow), Some(2000));
    assert_eq!(answer(&mut readout, other, counted(3)), Some(3), "the waiting change encodes next");
    assert_eq!(encodes.load(Ordering::SeqCst), 3);

    let edited = Key { revision: 4, ..key };
    assert_eq!(readout.update(edited, || || Some(7), || {}), None, "an edited document shows no size of the old one");
    assert_eq!(answer(&mut readout, edited, || Some(7)), Some(7), "an edited document is encoded afresh");
}

/// Going back to settings already worked out shows their size at once, without encoding again;
/// only the last [`KEPT`] are kept.
#[test]
fn earlier_settings_are_not_encoded_again() {
    let encodes = Arc::new(AtomicU32::new(0));
    let counted = |size: u64| {
        let encodes = encodes.clone();
        move || {
            encodes.fetch_add(1, Ordering::SeqCst);
            Some(size)
        }
    };
    let at = |quality: u8| Key { doc: photocraft_doc::DocId(1), revision: 3, quality, format: JpegFormat::Baseline };
    let mut readout = Readout::default();
    assert_eq!(answer(&mut readout, at(84), counted(1000)), Some(1000));
    assert_eq!(answer(&mut readout, at(92), counted(2000)), Some(2000));
    assert_eq!(readout.update(at(84), || counted(1), || {}), Some(Some(1000)), "back to 84: its size, at once");
    assert_eq!(readout.update(at(92), || counted(1), || {}), Some(Some(2000)));
    assert_eq!(encodes.load(Ordering::SeqCst), 2);

    for q in 1..=KEPT as u8 {
        assert_eq!(answer(&mut readout, at(q), counted(u64::from(q))), Some(u64::from(q)));
    }
    assert_eq!(encodes.load(Ordering::SeqCst), 2 + KEPT as u32);
    assert_eq!(answer(&mut readout, at(84), counted(1000)), Some(1000), "the oldest sizes make room");
    assert_eq!(encodes.load(Ordering::SeqCst), 3 + KEPT as u32);
}

/// The hosted dialog shows the size its settings write in its footer: the request the dialog
/// makes is answered from the worker's encode of the parked document, over several passes.
#[test]
fn the_dialog_shows_the_size_it_will_write() {
    use craft::ui::dialog::StackDialog as _;
    use craft::ui::patterns::form_dialogs::HostedFormatOptions;
    use std::cell::RefCell;

    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
    app.sync_views();
    // The footer's words come from the dialog's own `Env`, read where the dialog draws.
    let wanted: Rc<RefCell<Option<u64>>> = Rc::default();
    let expected: Rc<RefCell<Option<String>>> = Rc::default();
    let (wanted_in, expected_in) = (wanted.clone(), expected.clone());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_context(app.ctx().clone()).build_ui_state(
        move |ui, app| {
            crate::dialogs::run(app, ui);
            if let Some(bytes) = *wanted_in.borrow() {
                *expected_in.borrow_mut() = Some(craft::ui::runtime::env::Env::of(ui).units().file_size(bytes));
            }
        },
        app,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::AppTheme::pro());
    super::super::park(h.state_mut(), "p.jpg".into(), false, "file.saveAs").unwrap();
    h.run_steps(1);
    let quality = h.state().framework.dialogs.get::<HostedFormatOptions>().and_then(|d| d.json_fields()).and_then(|f| f["quality"].as_f64()).expect("the dialog has a quality") as u8;
    let src = Source::of(h.state_mut()).unwrap();
    *wanted.borrow_mut() = encoded_size(&src.doc, &src.settings(quality, JpegFormat::Baseline));
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        h.run_steps(1);
        if let Some(text) = expected.borrow().as_deref() {
            if h.query_by_label(text).is_some() {
                break;
            }
        }
        assert!(Instant::now() < deadline, "JPEG Options never showed the size ({:?})", expected.borrow());
        std::thread::sleep(Duration::from_millis(5));
    }
}
