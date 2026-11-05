use super::*;
use craft::host::PickKind;
use photocraft_engine::Session;
use photocraft_format::list_recovery;
use photocraft_ui_egui::file_picks::filters::{OPEN_EXTS, OPEN_FILTERS, SAVE_FILTERS, netpbm_as_pnm, save_filters};
use photocraft_ui_egui::{PhotocraftApp, prefs_ui};
use serde_json::{Value, json};

/// Regression using the real clipboard on an isolated Windows CI runner.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires exclusive use of the Windows clipboard"]
fn busy_native_clipboard_preserves_copied_shape() {
    for id in ["edit.paste", "edit.pasteSpecial.pasteInPlace"] {
        let mut native = native(None);
        let services =
            Services { clipboard_set_image: native.clipboard_set_image.take(), clipboard_get_image: native.clipboard_get_image.take(), ..Default::default() };
        let mut app = PhotocraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width": 80, "height": 60})).unwrap();
        app.run("shape.create", json!({"kind": "rect", "rect": [10, 10, 40, 20], "fill": "#ff0000"})).unwrap();
        app.run("edit.copy", json!({})).unwrap();
        assert!(app.session.clipboard.as_ref().unwrap().layers.is_some());
        assert!(app.services.clipboard_get_image.as_mut().unwrap()().unwrap().is_some(), "native export succeeded");
        app.run("file.new", json!({"width": 80, "height": 60})).unwrap();

        // Another thread holds the OS clipboard open beyond arboard's bounded retry window.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let holder = std::thread::spawn(move || {
            // A non-null window distinguishes this owner from arboard's null-window opens.
            let guard = winsafe::HWND::GetDesktopWindow().OpenClipboard().unwrap();
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
            drop(guard);
        });
        ready_rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
        let contention = arboard::Clipboard::new().unwrap().get_image().err();
        let first = app.run(id, json!({}));
        release_tx.send(()).unwrap();
        holder.join().unwrap();
        assert!(matches!(contention, Some(arboard::Error::ClipboardOccupied)), "diagnostic must actually occupy the clipboard: {contention:?}");
        eprintln!("paste while clipboard occupied: {first:?}");
        assert!(first.is_err(), "report contention rather than a successful no-op");
        assert!(
            app.session.clipboard.as_ref().is_some_and(|clip| clip.layers.is_some()),
            "a temporary OS read failure must not discard the copied editable shape"
        );
        app.run(id, json!({})).unwrap();
        let state = app.session.active().unwrap();
        assert!(
            matches!(state.doc.layer(state.active_layer.unwrap()).unwrap().content, photocraft_doc::LayerContent::Shape(_)),
            "retry keeps the shape editable"
        );
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
#[cfg(not(all(unix, not(target_os = "macos"))))]
#[test]
fn native_services_provide_update_transport() {
    assert!(native(None).check_updates.is_some());
}

#[test]
fn xwayland_command_matches_how_photocraft_was_installed() {
    // Flatpak: WAYLAND_DISPLAY doesn't reach the sandbox's socket choice; flatpak's flags do.
    let flatpak = xwayland_command(Some("ai.storyteller.photocraft"), None, false);
    assert_eq!(flatpak.as_deref(), Some("flatpak run --nosocket=wayland --socket=x11 ai.storyteller.photocraft"));
    // AppImage: its own path, quoted for the shell (spaces, quotes).
    let appimage = xwayland_command(None, Some("/home/me/My Apps/it's.AppImage"), true);
    assert_eq!(appimage.as_deref(), Some(r"WAYLAND_DISPLAY= '/home/me/My Apps/it'\''s.AppImage'"));
    let bare = xwayland_command(None, Some("/opt/photocraft-0.3.0-linux-x86_64.AppImage"), true);
    assert_eq!(bare.as_deref(), Some("WAYLAND_DISPLAY= /opt/photocraft-0.3.0-linux-x86_64.AppImage"));
    // deb, rpm, AUR, tarball: the binary on the PATH.
    assert_eq!(xwayland_command(None, None, true).as_deref(), Some("WAYLAND_DISPLAY= photocraft"));
    assert_eq!(xwayland_command(Some(""), Some(""), true).as_deref(), Some("WAYLAND_DISPLAY= photocraft"));
    // No X server (XWayland disabled): nothing to suggest.
    assert_eq!(xwayland_command(None, Some("/opt/p.AppImage"), false), None);
}

/// Tests every native save dialog and records the suggested file name. Each name's file type must come first
/// in the save panel, or the panel appends the first type's extension (`photo.webp.psd`, `photo.gif.psd`).
#[test]
fn every_save_dialog_leads_with_its_own_extension() {
    // Every save dialog is answered with Cancel, as the user would; the requests are kept.
    let files = std::sync::Arc::new(photocraft_ui_egui::file_picks::scripted::ScriptedFiles::new());
    let services = Services { files: photocraft_ui_egui::file_picks::scripted::service(&files), ..Default::default() };
    let mut app = PhotocraftApp::new(Session::new(), services);
    let ctx = egui::Context::default();
    // Dialogs are shown on the next frame: poll once so each is answered before the next asks.
    let invoke = |app: &mut PhotocraftApp, id: &str| {
        let r = photocraft_ui_egui::menus::invoke(app, &ctx, id, json!({}));
        app.poll_file_dialog(&ctx, None);
        r
    };
    let dialog = |app: &mut PhotocraftApp, id: &str, fields: Value| {
        let d = invoke(app, id).unwrap()["dialog"].as_u64().unwrap();
        for (k, v) in fields.as_object().unwrap() {
            app.ui.dialog_mut(d).unwrap().fields.insert(k.clone(), v.clone());
        }
        let _ = photocraft_ui_egui::dialogs::confirm(app, d);
        app.poll_file_dialog(&ctx, None);
    };
    new_doc(&mut app, "#ff0000");
    // Save As, Save a Copy.
    // Files in a format Save As can't write, like .dng, should suggest .psd instead.
    let _ = invoke(&mut app, "file.saveAs");
    for name in ["cat.pcraft", "cat.jpeg", "cat.gif", "cat.bmp", "cat.dng"] {
        app.session.active_mut().unwrap().path = Some(name.into());
        let _ = invoke(&mut app, "file.saveAs");
    }
    let _ = invoke(&mut app, "file.saveACopy");
    // Export As, Quick Export, Save for Web (with and without slices).
    for format in ["png", "jpg", "webp", "tif", "tga"] {
        dialog(&mut app, "file.export.exportAs", json!({"format": format}));
    }
    for format in ["png", "jpg", "gif", "webp"] {
        app.run("prefs.set", json!({"path": "export.quickExportFormat", "value": format})).unwrap();
        let _ = invoke(&mut app, "file.export.quickExportAsPng");
    }
    for format in ["gif", "png8", "png24", "jpeg", "webp", "wbmp"] {
        dialog(&mut app, "file.export.saveForWebLegacy", json!({"format": format}));
    }
    app.run("slice.new", json!({"x": 0, "y": 0, "width": 2, "height": 2})).unwrap();
    dialog(&mut app, "file.export.saveForWebLegacy", json!({"html": true}));
    // Measurement Log, Export/Import Presets.
    app.run("image.analysis.recordMeasurements", json!({})).unwrap();
    let _ = invoke(&mut app, "measurementLog.export");
    dialog(&mut app, "edit.presets.exportImportPresets", json!({"action": "export"}));
    let asked: Vec<_> = files.requests().into_iter().filter(|r| r.kind == PickKind::Save).collect();
    let exts: Vec<&str> = asked.iter().filter_map(|r| r.file_name.as_deref().and_then(|s| s.rsplit_once('.')).map(|(_, e)| e)).collect();
    // The extension each save above should suggest, in order.
    let want = [
        "psd",                            // Save As untitled
        "pcraft jpeg gif bmp psd",        // Save As opened .pcraft .jpeg .gif .bmp .dng
        "psd",                            // Save a Copy
        "png jpg webp tif tga",           // Export As
        "png jpg gif webp",               // Quick Export
        "gif png png jpg webp wbmp html", // Save for Web: gif png8 png24 jpeg webp wbmp, then with slices
        "csv pcpresets",                  // Measurement Log, Export Presets
    ]
    .join(" ");
    assert_eq!(exts, want.split(' ').collect::<Vec<_>>());
    for (request, ext) in asked.iter().zip(exts) {
        // The first type's first extension is the one Windows adds to the bare name.
        assert_eq!(request.filters[0].extensions.first().map(String::as_str), Some(ext), "{:?}: {:?}", request.file_name, request.filters[0]);
    }
    assert_eq!(save_filters("Untitled")[0].0, "PSD Document", "no extension keeps the default");
}

/// Every flat format Save As offers is one PhotoCraft writes, and ICO is among them (#4357).
#[test]
fn save_as_offers_only_writable_formats_including_ico() {
    // Layered formats, and PDF, are written by `photocraft-io`, not a codec.
    let layered = ["psd", "psb", "pcraft", "ora", "pdf"];
    for (name, exts) in SAVE_FILTERS {
        for ext in exts.iter().filter(|e| !layered.contains(e)) {
            let format = photocraft_codecs::from_extension(ext).unwrap_or_else(|| panic!("{name}: .{ext} is no known format"));
            assert!(format.caps().write, "{name}: .{ext} can't be written");
        }
    }
    for ext in ["ico", "gif", "bmp", "dds", "pnm", "hdr", "dib", "jpeg", "tiff"] {
        assert!(save_filters("Untitled-1.psd").iter().any(|(_, exts)| exts.iter().any(|e| e == ext)), ".{ext} is offered");
    }
    // The codec writes P5/P6/P7/PF from the image, never from the name, so Netpbm offers only the
    // neutral .pnm (#3609; CodeRabbit on #9775): a .pbm, .pgm or .pfm name would hold other data.
    let netpbm = save_filters("Untitled").into_iter().find(|(name, _)| name == "Netpbm").map(|(_, exts)| exts);
    assert_eq!(netpbm, Some(vec!["pnm".to_string()]));
    // The suggested name itself becomes .pnm, as macOS and rfd keep it (CodeRabbit on #9775).
    assert_eq!(netpbm_as_pnm("scans/image.pgm"), Path::new("scans/image.pnm").to_string_lossy());
    assert_eq!(netpbm_as_pnm("image.PFM"), "image.pnm");
    assert_eq!(netpbm_as_pnm("image.png"), "image.png");
    assert_eq!(netpbm_as_pnm("Untitled-1"), "Untitled-1");
    // Also when Save As starts from a subtype's name: no filter offers that subtype again.
    for name in ["image.pbm", "image.pgm", "image.ppm", "image.pam", "image.pfm"] {
        let filters = save_filters(name);
        assert_eq!(filters[0], ("Netpbm".to_string(), vec!["pnm".to_string()]), "{name}");
        let subtype = name.rsplit('.').next().unwrap_or_default();
        assert!(!filters.iter().any(|(_, exts)| exts.iter().any(|e| e == subtype)), "{name}: .{subtype} is offered");
    }
    // File › Open lists every file Save As writes, also in that format's own type (PDF is written
    // but not read yet).
    for (name, exts) in SAVE_FILTERS {
        for ext in exts.iter().filter(|e| **e != "pdf") {
            assert!(OPEN_EXTS.contains(ext), "{name}: Open's All Formats lacks .{ext}");
            assert!(OPEN_FILTERS.iter().any(|(_, open)| open.contains(ext)), "{name}: no Open type lists .{ext}");
        }
    }
    assert_eq!(save_filters("icon.ico")[0].0, "ICO", "an .ico name leads with its own type");
    // What Save As writes there: an icon up to 256×256, and a clear error past that.
    let doc = |side: u32| {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": side, "height": side})).unwrap();
        s.active().unwrap().doc.clone()
    };
    let opts = export_options(&photocraft_ui_egui::ExportSettings::default());
    let icon = photocraft_io::export(&doc(256), "icon.ico", &opts).unwrap();
    assert_eq!(icon.bytes[..4], [0, 0, 1, 0], "an ICO header");
    // And what the Netpbm and Radiance rows write.
    let pnm = photocraft_io::export(&doc(4), "x.pnm", &opts).unwrap();
    assert_eq!(pnm.bytes[..2], *b"P6", "an RGB document saves as a binary pixmap");
    let hdr = photocraft_io::export(&doc(4), "x.hdr", &opts).unwrap();
    assert!(hdr.bytes.starts_with(b"#?RADIANCE"), "a Radiance header");
    let err = photocraft_io::export(&doc(300), "icon.ico", &opts).map(|_| ()).unwrap_err().to_string();
    assert!(err.contains("256"), "{err}");
}

#[test]
fn block_on_waits_for_a_wake_from_another_thread() {
    let (tx, rx) = std::sync::mpsc::channel::<std::task::Waker>();
    let mut woken = false;
    let ready = std::future::poll_fn(move |cx| {
        if woken {
            return std::task::Poll::Ready(7);
        }
        woken = true;
        let _ = tx.send(cx.waker().clone());
        std::task::Poll::Pending
    });
    let waker = std::thread::spawn(move || rx.recv().unwrap().wake());
    assert_eq!(block_on(ready), 7);
    waker.join().unwrap();
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

/// One app launch with crash recovery in `dir` (dropping it is the crash: background writes
/// already queued finish, nothing else runs).
fn launch(dir: &Path) -> PhotocraftApp {
    let mut app = PhotocraftApp::new(Session::new(), recovery_services(Some(dir.to_path_buf())));
    let ctx = egui::Context::default();
    prefs_ui::tick(&mut app, &ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.session.has_jobs() && std::time::Instant::now() < deadline {
        photocraft_ui_egui::jobs_ui::tick(&mut app, &ctx);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(!app.session.has_jobs(), "recovery finished");
    app
}

/// The first pixel of each open document, and whether it's unsaved.
fn open_docs(app: &PhotocraftApp) -> Vec<([f32; 4], bool)> {
    app.session.documents().iter().map(|d| (photocraft_compose::flatten(&d.doc).px.first().copied().unwrap_or_default(), d.is_dirty())).collect()
}

fn index_of(app: &PhotocraftApp, color: [f32; 4]) -> usize {
    open_docs(app).iter().position(|(c, _)| *c == color).unwrap()
}

fn new_doc(app: &mut PhotocraftApp, color: &str) {
    app.run("file.new", json!({"width": 4, "height": 4})).unwrap();
    app.run("edit.fill", json!({"color": color})).unwrap();
}

fn autosave(app: &mut PhotocraftApp, ctx: &egui::Context) {
    prefs_ui::autosave_now(app);
    prefs_ui::tick(app, ctx);
}

#[test]
fn recovered_documents_survive_a_second_crash_until_saved_or_closed() {
    let dir = std::env::temp_dir().join(format!("photocraft-recovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let ctx = egui::Context::default();
    // Launch 1: two unsaved documents are autosaved, then the app crashes.
    {
        let mut app = launch(&dir);
        new_doc(&mut app, "#ff0000");
        new_doc(&mut app, "#0000ff");
        autosave(&mut app, &ctx);
    }
    assert_eq!(list_recovery(&dir).len(), 2);
    // Launch 2 recovers both and crashes again before its first autosave (#444).
    {
        let mut app = launch(&dir);
        let mut docs = open_docs(&app);
        docs.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
        assert_eq!(docs, [(BLUE, true), (RED, true)], "recovered and still unsaved");
        prefs_ui::tick(&mut app, &ctx);
    }
    assert_eq!(list_recovery(&dir).len(), 2, "recovering keeps the entries on disk");
    // Launch 3 has both again; autosaving unchanged documents adds nothing, a new document
    // (whose id may repeat one from an earlier launch) gets an entry of its own.
    {
        let mut app = launch(&dir);
        assert_eq!(app.session.documents().len(), 2);
        autosave(&mut app, &ctx);
        new_doc(&mut app, "#00ff00");
        autosave(&mut app, &ctx);
    }
    assert_eq!(list_recovery(&dir).len(), 3);
    // Launch 4: closing and saving recovered documents removes their entries, no duplicates.
    let mut app = launch(&dir);
    assert_eq!(app.session.documents().len(), 3);
    let red = index_of(&app, RED);
    app.run("file.close", json!({"document": red})).unwrap();
    // Saving marks the revision saved (the file write itself is the save service's job).
    assert!(app.session.set_active(index_of(&app, BLUE)));
    let st = app.session.active_mut().unwrap();
    st.saved_revision = st.revision;
    prefs_ui::tick(&mut app, &ctx);
    let left = list_recovery(&dir);
    assert_eq!(left.len(), 1);
    assert_eq!(photocraft_compose::flatten(&photocraft_format::recover(&left[0]).unwrap()).px.first().copied(), Some(GREEN));
    let green = index_of(&app, GREEN);
    app.run("file.close", json!({"document": green})).unwrap();
    prefs_ui::tick(&mut app, &ctx);
    assert!(list_recovery(&dir).is_empty());
    drop(app);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn failed_background_autosave_is_reported_and_retried_without_another_edit() {
    let dir = temp("autosave-retry");
    let recovery = dir.join("Recovery");
    // A regular file prevents creation of the recovery bundle directory.
    std::fs::write(&recovery, b"blocked").unwrap();
    let ctx = egui::Context::default();
    let mut app = launch(&recovery);
    new_doc(&mut app, "#ff0000");
    let revision = app.session.active().unwrap().revision;
    assert_ne!(revision, 0, "a sidecar-less bundle lists as revision 0");
    autosave(&mut app, &ctx);

    let mut failed = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        prefs_ui::tick(&mut app, &ctx);
        if app.ui.status.starts_with("Autosave failed:") {
            failed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(failed, "background write failures must reach the UI");
    assert!(app.ui.status_error);
    assert!(list_recovery(&recovery).is_empty());

    // No edit occurs: retrying this very same revision must still work.
    std::fs::remove_file(&recovery).unwrap();
    std::fs::create_dir(&recovery).unwrap();
    autosave(&mut app, &ctx);
    // The worker commits the bundle's manifest before it publishes the sidecar, and
    // `list_recovery` lists a committed sidecar-less bundle (revision 0) on its own, so
    // the first non-empty listing can come from the middle of the write. Wait for the write
    // to finish: the entry then carries the sidecar's revision.
    let mut recovered = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        prefs_ui::tick(&mut app, &ctx);
        recovered = list_recovery(&recovery);
        if recovered.iter().any(|e| e.info.revision == revision) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(recovered.len(), 1, "unchanged revision should retry after failure");
    assert_eq!(recovered[0].info.revision, revision);
    let restored = photocraft_format::recover(&recovered[0]).unwrap();
    assert_eq!(photocraft_compose::flatten(&restored).px.first().copied(), Some(RED));
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

use std::sync::atomic::{AtomicUsize, Ordering};

fn temp(tag: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!("photocraft-services-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn red_png(w: u32, h: u32) -> Vec<u8> {
    let img = Image::from_u8(w, h, ChannelLayout::Rgba, [255u8, 0, 0, 255].repeat((w * h) as usize)).unwrap();
    photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &EncodeOptions::default()).unwrap()
}

/// A 1×1 uncompressed 32-bit TGA (TGA has no magic number).
fn tga_1x1() -> Vec<u8> {
    let mut b = vec![0u8, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 32, 8];
    b.extend_from_slice(&[0, 0, 255, 255]); // BGRA red
    b
}

#[test]
fn copied_image_file_pastes_its_pixels() {
    let dir = temp("png");
    let png = dir.join("red.png");
    std::fs::write(&png, red_png(3, 2)).unwrap();
    let (w, h, px) = image_from_files(&[png]).unwrap();
    assert_eq!((w, h), (3, 2));
    assert_eq!(px, [255u8, 0, 0, 255].repeat(6));
}

#[test]
fn crlf_from_a_uri_list_is_ignored() {
    // arboard splits text/uri-list on LF, so GNOME Files' CRLF list leaves a '\r' behind.
    let dir = temp("crlf");
    let png = dir.join("red image.png");
    std::fs::write(&png, red_png(2, 2)).unwrap();
    let with_cr = PathBuf::from(format!("{}\r", png.display()));
    assert_eq!(image_from_files(&[with_cr]).map(|(w, h, _)| (w, h)), Some((2, 2)));
}

#[test]
fn non_images_are_skipped_for_the_first_image() {
    let dir = temp("mixed");
    let (txt, png) = (dir.join("notes.txt"), dir.join("red.png"));
    std::fs::write(&txt, b"not an image").unwrap();
    std::fs::write(&png, red_png(4, 1)).unwrap();
    let missing = dir.join("gone.png");
    assert_eq!(image_from_files(&[missing.clone(), txt.clone(), dir.clone(), png]).map(|(w, h, _)| (w, h)), Some((4, 1)));
    assert!(image_from_files(&[missing, txt, dir]).is_none());
    assert!(image_from_files(&[]).is_none());
}

#[test]
fn a_damaged_image_is_skipped_not_a_panic() {
    let dir = temp("damaged");
    let bad = dir.join("cut.png");
    std::fs::write(&bad, &red_png(8, 8)[..20]).unwrap();
    assert!(image_from_files(&[bad]).is_none());
}

fn encoded(format: photocraft_codecs::Format, w: u32, h: u32) -> Vec<u8> {
    let img = Image::from_u8(w, h, ChannelLayout::Rgba, [255u8, 0, 0, 255].repeat((w * h) as usize)).unwrap();
    photocraft_codecs::encode(&img, format, &EncodeOptions::default()).unwrap()
}

#[test]
fn non_png_clipboard_images_paste() {
    // #2546: a screenshot tool that offers only image/bmp or image/jpeg pasted nothing.
    let bmp = encoded(photocraft_codecs::Format::Bmp, 3, 2);
    let offers = vec!["text/plain".to_string(), "image/bmp".to_string()];
    let mut asked = Vec::new();
    let got = image_from_offers(&offers, |mime| {
        asked.push(mime.to_string());
        Some(bmp.clone())
    });
    assert_eq!(got, Some((3, 2, [255u8, 0, 0, 255].repeat(6))));
    assert_eq!(asked, ["image/bmp"], "text offers are never fetched");

    let jpeg = encoded(photocraft_codecs::Format::Jpeg, 16, 8);
    let offers = vec!["image/jpeg".to_string()];
    assert_eq!(image_from_offers(&offers, |_| Some(jpeg.clone())).map(|(w, h, _)| (w, h)), Some((16, 8)));
}

#[test]
fn undecodable_clipboard_offers_fall_through_to_the_next() {
    let bmp = encoded(photocraft_codecs::Format::Bmp, 2, 2);
    let cut = encoded(photocraft_codecs::Format::Png, 8, 8)[..20].to_vec();
    let offers: Vec<String> = ["image/webp", "image/png", "image/x-tga", "image/bmp"].map(String::from).to_vec();
    let got = image_from_offers(&offers, |mime| match mime {
        "image/webp" => None,
        "image/png" => Some(cut.clone()),
        "image/x-tga" => Some(b"not an image".to_vec()),
        _ => Some(bmp.clone()),
    });
    assert_eq!(got.map(|(w, h, _)| (w, h)), Some((2, 2)));
    assert!(image_from_offers(&offers[..3], |_| Some(cut.clone())).is_none());
    assert!(image_from_offers(&[], |_| Some(bmp.clone())).is_none());
}

#[test]
fn clipboard_tga_is_trusted_only_by_its_type() {
    assert_eq!(image_from_offers(&["image/x-tga".to_string()], |_| Some(tga_1x1())), Some((1, 1, vec![255, 0, 0, 255])));
    assert!(image_from_offers(&["image/x-unknown".to_string()], |_| Some(tga_1x1())).is_none());
}

#[test]
fn tga_is_trusted_only_with_its_extension() {
    let dir = temp("tga");
    let (tga, bin) = (dir.join("red.tga"), dir.join("red.bin"));
    std::fs::write(&tga, tga_1x1()).unwrap();
    std::fs::write(&bin, tga_1x1()).unwrap();
    assert_eq!(image_from_files(&[tga]), Some((1, 1, vec![255, 0, 0, 255])));
    assert!(image_from_files(&[bin]).is_none());
}

/// A 121 × 81 white rectangle with a 1-unit black stroke, written as markup the way a vector
/// app's Copy puts it on the clipboard as text.
const COPIED_RECTANGLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="121" height="81" viewBox="0 0 121 81">
  <g><path d="M0.5 0.5 L120.5 0.5 L120.5 80.5 L0.5 80.5 Z" fill="#ffffff" stroke="#000000"/></g>
</svg>
"##;

/// A 40 × 20 drawing whose left half is a red square; the right half has no art.
const HALF_RED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="20" height="20" fill="#ff0000"/></svg>"##;

/// The clipboard as `clipboard_image` reads it when it holds only `text`.
fn from_text(text: &str) -> Option<(u32, u32, Vec<u8>)> {
    clipboard_image(&mut (), |_| Ok(None), |_| Ok(None), |_| Ok(Some(text.to_string()))).expect("no read fails")
}

fn pixel(img: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 4] {
    let at = ((y * img.0 + x) * 4) as usize;
    img.2[at..at + 4].try_into().unwrap()
}

#[test]
fn svg_markup_copied_as_text_pastes_its_pixels() {
    let img = from_text(COPIED_RECTANGLE).expect("the drawing's pixels");
    assert_eq!((img.0, img.1, img.2.len()), (121, 81, 121 * 81 * 4), "one pixel per SVG unit");
    assert_eq!(pixel(&img, 60, 40), [255, 255, 255, 255], "the white fill");
    for (x, y) in [(0, 40), (120, 40), (60, 0), (60, 80)] {
        let [r, g, b, a] = pixel(&img, x, y);
        assert!(a == 255 && r < 8 && g < 8 && b < 8, "the black stroke at {x},{y}: {:?}", [r, g, b, a]);
    }
    let img = from_text(HALF_RED).expect("the drawing's pixels");
    assert_eq!((img.0, img.1), (40, 20));
    assert_eq!(pixel(&img, 5, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(&img, 30, 10), [0, 0, 0, 0], "transparent where there is no art");
}

#[test]
fn text_that_is_not_a_drawing_pastes_nothing() {
    for text in ["", "hello", "<p>not a drawing</p>", "<?xml version=\"1.0\"?><x:xmpmeta xmlns:x=\"adobe:ns:meta/\"/>"] {
        assert!(from_text(text).is_none(), "{text:?}");
    }
    // Markup that doesn't parse, and markup past the read limit, paste nothing either.
    assert!(from_text("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect").is_none());
    assert!(from_text("<svg width=\"8\" height=\"8\"><g></svg>").is_none());
    let mut huge = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\">");
    huge.push_str(&" ".repeat(usize::try_from(MAX_CLIPBOARD_SVG_BYTES).unwrap()));
    huge.push_str("</svg>");
    assert!(from_text(&huge).is_none(), "markup past the limit isn't parsed");
}

#[test]
fn svg_clipboard_text_accepts_a_long_xml_preamble() {
    let padding = "x".repeat(8192);
    for prefix in [format!("<!--{padding}-->"), format!("<?xml version=\"1.0\"?>\n<!--{padding}-->"), format!("<?drawing {padding}?>")] {
        let text = format!("{prefix}{HALF_RED}");
        let img = from_text(&text).expect("SVG after the XML preamble");
        assert_eq!((img.0, img.1), (40, 20));
        assert_eq!(pixel(&img, 5, 10), [255, 0, 0, 255]);
        assert_eq!(pixel(&img, 30, 10), [0, 0, 0, 0]);
    }
    assert!(from_text(&format!("<!--{padding}<svg/>--><not-svg/>")).is_none());
    assert!(from_text(&format!("<!--{padding}--><svg><g></svg>")).is_none());
}

#[test]
fn a_drawing_past_the_pixel_limit_pastes_scaled_down() {
    let wide = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="10000"><rect width="20000" height="10000" fill="#0000ff"/></svg>"##;
    let img = from_text(wide).expect("scaled down, not dropped");
    let pixels = u64::from(img.0) * u64::from(img.1);
    assert!(pixels <= 24_000_000 && pixels > 12_000_000, "{} × {}", img.0, img.1);
    assert!(img.0.abs_diff(img.1 * 2) <= 2, "the proportions are kept: {} × {}", img.0, img.1);
    assert_eq!(pixel(&img, img.0 / 2, img.1 / 2), [0, 0, 255, 255]);
}

/// Copied files and an image come before SVG text, which is read only when neither is there.
#[test]
fn copied_files_and_images_come_before_svg_text() {
    let dir = temp("svg-order");
    let png = dir.join("red.png");
    std::fs::write(&png, red_png(3, 2)).unwrap();
    let blue = (2, 2, [0u8, 0, 255, 255].repeat(4));
    let mut text_read = false;
    let svg_text = |read: &mut bool| {
        *read = true;
        Ok(Some(COPIED_RECTANGLE.to_string()))
    };
    let got = clipboard_image(&mut text_read, |_| Ok(Some(vec![png.clone()])), |_| Ok(Some(blue.clone())), svg_text);
    assert_eq!(got.unwrap().map(|(w, h, _)| (w, h)), Some((3, 2)), "the copied file");
    let got = clipboard_image(&mut text_read, |_| Ok(None), |_| Ok(Some(blue.clone())), svg_text);
    assert_eq!(got.unwrap().map(|(w, h, _)| (w, h)), Some((2, 2)), "the image, not the SVG text");
    assert!(!text_read, "the text isn't read while there is a file or an image");
    // A copied file that isn't an image (an .svg) falls through to the image, then the text.
    let svg = dir.join("drawing.svg");
    std::fs::write(&svg, COPIED_RECTANGLE).unwrap();
    let got = clipboard_image(&mut (), |_| Ok(Some(vec![svg.clone()])), |_| Ok(None), |_| Ok(Some(COPIED_RECTANGLE.to_string())));
    assert_eq!(got.unwrap().map(|(w, h, _)| (w, h)), Some((121, 81)));
}

/// A failed read, such as one of a busy clipboard, is the paste's error: the reads after it,
/// the SVG text included, don't run.
#[test]
fn a_failed_clipboard_read_is_an_error_and_the_text_is_not_read() {
    fn busy<T>() -> Result<T, String> {
        Err("the clipboard is busy".to_string())
    }
    let mut text_read = false;
    let svg_text = |read: &mut bool| {
        *read = true;
        Ok(Some(COPIED_RECTANGLE.to_string()))
    };
    let got = clipboard_image(&mut text_read, |_| busy(), |_| Ok(None), svg_text);
    assert_eq!(got, Err("the clipboard is busy".to_string()), "the file list's error");
    let got = clipboard_image(&mut text_read, |_| Ok(None), |_| busy(), svg_text);
    assert_eq!(got, Err("the clipboard is busy".to_string()), "the image's error");
    assert!(!text_read, "the text isn't read after a failed read");
    let got = clipboard_image(&mut (), |_| Ok(None), |_| Ok(None), |_| busy());
    assert_eq!(got, Err("the clipboard is busy".to_string()), "the text's error");
}

/// Only a clipboard with nothing in the format asked for reads as nothing to paste. A read
/// that fails in any other way, such as a broken pipe on Wayland, is the paste's error, for
/// the copied files and the text alike. Text that isn't UTF-8 reads as no SVG markup.
#[test]
fn only_missing_content_reads_as_nothing_to_paste() {
    use arboard::Error;
    let failures = || [Error::ClipboardOccupied, Error::ClipboardNotSupported, Error::Unknown { description: "broken pipe".to_string() }];
    assert_eq!(clipboard_read(Ok::<_, Error>(vec![PathBuf::from("a.png")])), Ok(Some(vec![PathBuf::from("a.png")])));
    assert_eq!(clipboard_read::<Vec<PathBuf>>(Err(Error::ContentNotAvailable)), Ok(None));
    for error in failures().into_iter().chain([Error::ConversionFailure]) {
        let message = error.to_string();
        assert_eq!(clipboard_read::<Vec<PathBuf>>(Err(error)), Err(message.clone()), "{message}");
    }
    assert_eq!(clipboard_text(Ok("words".to_string())), Ok(Some("words".to_string())));
    assert_eq!(clipboard_text(Err(Error::ContentNotAvailable)), Ok(None));
    assert_eq!(clipboard_text(Err(Error::ConversionFailure)), Ok(None), "text that isn't UTF-8");
    for error in failures() {
        let message = error.to_string();
        assert_eq!(clipboard_text(Err(error)), Err(message.clone()), "{message}");
    }
}

/// Edit › Paste with SVG markup as the clipboard's only content adds a new layer that holds the
/// drawing's art; other text is still a quiet "nothing to paste".
#[test]
fn paste_turns_svg_text_on_the_clipboard_into_a_layer() {
    let text: Rc<RefCell<String>> = Rc::new(RefCell::new(HALF_RED.to_string()));
    let clip = text.clone();
    let services = Services {
        clipboard_get_image: Some(Box::new(move || clipboard_image(&mut (), |_| Ok(None), |_| Ok(None), |_| Ok(Some(clip.borrow().clone()))))),
        ..Default::default()
    };
    let mut app = PhotocraftApp::new(Session::new(), services);
    let ctx = egui::Context::default();
    app.run("file.new", json!({"width": 300, "height": 200})).unwrap();
    let layers = app.session.active().unwrap().doc.layers.len();
    let r = photocraft_ui_egui::menus::invoke(&mut app, &ctx, "edit.paste", json!({})).unwrap();
    assert_ne!(r["pasted"], json!(false), "{r}");
    let st = app.session.active().unwrap();
    assert_eq!(st.doc.layers.len(), layers + 1);
    let surface = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
    let bounds = surface.content_bounds();
    assert_eq!((bounds.width(), bounds.height()), (20, 20), "the red half; the transparent half adds nothing");
    *text.borrow_mut() = "just words".to_string();
    let r = photocraft_ui_egui::menus::invoke(&mut app, &ctx, "edit.paste", json!({})).unwrap();
    assert_eq!(r["pasted"], json!(false));
    assert_eq!(app.ui.status, "Nothing to paste: the clipboard holds no image");
    assert_eq!(app.session.active().unwrap().doc.layers.len(), layers + 1);
}

/// #7582: a panic while reading the OS clipboard (in arboard, or decoding a copied file) is
/// the paste's error in the status bar, not a crash: the document stays and the next paste
/// works. The same for a panic while writing it.
#[test]
fn a_panicking_clipboard_read_is_reported_not_a_crash() {
    let panics = Rc::new(Cell::new(true));
    let p = panics.clone();
    let get = guarded_clipboard_get(move || if p.get() { panic!("clipboard decoder bug") } else { Ok(Some((2, 2, [0u8, 0, 255, 255].repeat(4)))) });
    let mut app = PhotocraftApp::new(Session::new(), Services { clipboard_get_image: Some(get), ..Default::default() });
    app.run("file.new", json!({"width": 8, "height": 8})).unwrap();
    let layers = app.session.active().unwrap().doc.layers.len();

    let error = app.run("edit.paste", json!({})).unwrap_err();
    assert!(error.contains("Paste failed"), "{error}");
    assert!(app.ui.status_error && app.ui.status.contains("Paste failed"), "{}", app.ui.status);
    assert_eq!(app.session.active().unwrap().doc.layers.len(), layers, "the document is unchanged");

    panics.set(false);
    app.run("edit.paste", json!({})).unwrap();
    assert_eq!(app.session.active().unwrap().doc.layers.len(), layers + 1, "the next paste works");

    let mut set = guarded_clipboard_set(|_: u32, _: u32, _: &[u8]| panic!("clipboard writer bug"));
    assert!(set(1, 1, &[0; 4]).unwrap_err().contains("Copy failed"));
}
