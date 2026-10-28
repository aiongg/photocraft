//! macOS window lifecycle: quitting (see [`terminate_later`]). All safe objc2 calls.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSWindowButton, NSWindowStyleMask};

/// Quit the way a Mac app does, with `-[NSApplication terminate:]` (`Services::quit`), instead of
/// letting eframe close the window. eframe drops the window while AppKit's run loop is still
/// running, and on Macs with a Touch Bar AppKit's next display cycle then tries to stop observing
/// a view that is gone: an uncaught exception that crashes (macOS 27) or leaves a windowless app
/// that only Force Quit ends (macOS 26) (#1458, #1575). `terminate:` ends the process before
/// another display cycle runs; winit's `applicationWillTerminate:` closes the windows and has
/// eframe save the window layout first, as the old ⌘Q (winit's default menu) did.
///
/// Scheduled on the main queue: winit has to be out of its event handler, or eframe's exit
/// handling (and the save) is dropped.
pub fn terminate_later() {
    dispatch2::DispatchQueue::main().exec_async(|| match MainThreadMarker::new() {
        Some(mtm) => NSApplication::sharedApplication(mtm).terminate(None),
        None => log::warn!("quit: the main queue ran off the main thread; not terminating"),
    });
}

/// Sets `movable` on the windows whose title bar the app draws over (a full-size content view
/// with visible window buttons), as `craft_window::title_bar::sync_movable` does from the
/// pointer. Floating panels in their own windows keep theirs.
pub fn set_movable(movable: bool) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    for window in NSApplication::sharedApplication(mtm).windows().to_vec() {
        let buttons = window.standardWindowButton(NSWindowButton::CloseButton).is_some_and(|close| !close.isHidden());
        if window.styleMask().contains(NSWindowStyleMask::FullSizeContentView) && buttons {
            window.setMovable(movable);
        }
    }
}
