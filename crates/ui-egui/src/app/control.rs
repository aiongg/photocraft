//! The control channel's UI-thread side (desktop only, as craft-automation's channel is):
//! craft-automation's `ControlChannel` hands each request to PhotoCraft's method table
//! (`crate::control::handle`) and settles its reply now, after the queued synthetic input, after a
//! job, or through its screenshot queue.

use std::sync::mpsc::Receiver;

use craft::automation::control::{ControlChannel, ControlRequest, ControlResponse};
use craft::automation::screenshot::{CaptureGeometry, DocumentId, settle_ms};
use craft::ui::runtime::motion;
use craft::ui::runtime::windows::WindowHost;

use super::PhotocraftApp;

impl PhotocraftApp {
    /// Attach a control channel (requests arrive from a transport thread: TCP, stdin, tests).
    pub fn with_control(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control = Some(ControlChannel::new(rx));
        self
    }

    /// Logic phase: finish or expire outstanding captures first, so a request admitted here sees
    /// their released budget, then run the queued requests.
    pub(crate) fn drain_control(&mut self, ctx: &egui::Context) {
        let Some(mut channel) = self.control.take() else { return };
        let document = self.document_identity();
        // begin_pass supplied the new RawInput before App::logic: observe the geometry before
        // any completed callback is collected.
        channel.captures.note_geometry(CaptureGeometry::from_facts(WindowHost::of(ctx).screenshot_facts(), document));
        let observed = channel.captures.last_geometry();
        let finished = channel.captures.drain(motion::now(), observed, document);
        self.finish_captures(&mut channel, finished);
        let settle = settle_ms(ctx.global_style().animation_time);
        while let Some(req) = channel.next_request() {
            let reply = crate::control::handle(self, ctx, &req);
            channel.settle(req, reply, self.document_identity(), settle);
            // `ui.set` may have changed the edit target: the next request sees its colours.
            self.sync_mask_targets();
        }
        self.control = Some(channel);
    }

    /// UI phase of a real pass (not a hidden logic tick): record the capture geometry, the
    /// document identity included, and count one settling pass.
    pub(super) fn note_capture_pass(&mut self, ctx: &egui::Context) {
        let document = self.document_identity();
        if let Some(channel) = self.control.as_mut() {
            channel.captures.note_geometry(CaptureGeometry::from_facts(WindowHost::of(ctx).screenshot_facts(), document));
            channel.captures.note_pass();
        }
    }

    /// Issue due screenshot requests, at the end of a UI pass whose state will be present: a
    /// discarded pass (popup sizing, `request_discard`) is re-run, so it captures nothing.
    pub(super) fn issue_captures(&mut self, ctx: &egui::Context) {
        if ctx.will_discard() {
            return;
        }
        let document = self.document_identity();
        let Some(mut channel) = self.control.take() else { return };
        let finished = channel.captures.issue(&WindowHost::of(ctx), motion::now(), document);
        self.finish_captures(&mut channel, finished);
        self.control = Some(channel);
    }

    /// Logic phase: poll while a capture is pending (the transport wakes the UI on arrival, so
    /// nothing else polls; polling every 50 ms made idle apps render at 20 fps). True while a
    /// capture waits to be issued: a hidden host runs no UI pass, so the caller asks for one.
    pub(super) fn poll_captures(&self, ctx: &egui::Context) -> bool {
        let Some(captures) = self.control.as_ref().map(|channel| &channel.captures) else { return false };
        if captures.any_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        captures.any_unissued()
    }

    /// Reply to finished screenshots; a PNG with a path goes through the automation write
    /// authority.
    fn finish_captures(&mut self, channel: &mut ControlChannel, finished: Vec<craft::automation::screenshot::Finished>) {
        let services = &mut self.services;
        channel.finish_captures(finished, &mut |path: &str, bytes: &[u8]| match services.automation_write.as_mut() {
            Some(write) => write(path, bytes),
            None => Err("automation write authority is not configured".into()),
        });
    }

    /// The synthetic input every waiting reply was queued behind has been processed.
    pub(super) fn reply_input_processed(&mut self) {
        if let Some(channel) = self.control.as_mut() {
            channel.input_processed();
        }
    }

    /// The synthetic input was dropped: fail the replies waiting on it.
    pub(super) fn reply_input_cancelled(&mut self, reason: &str) {
        if let Some(channel) = self.control.as_mut() {
            channel.input_cancelled(reason);
        }
    }

    /// Job `job` ended: `reply` answers the requests waiting on it.
    pub(crate) fn reply_job(&mut self, job: photocraft_engine::jobs::JobId, reply: &ControlResponse) {
        if let Some(channel) = self.control.as_mut() {
            channel.job_finished(job.0, reply);
        }
    }

    /// The app is closing: release pending screenshots' image budget and answer their requests;
    /// a late callback is inert (it holds only a weak slot).
    pub(super) fn close_control(&mut self) {
        let Some(mut channel) = self.control.take() else { return };
        let finished = channel.captures.cancel_all("the application is closing");
        self.finish_captures(&mut channel, finished);
    }

    /// The active document's identity at admission: a queued capture never applies to another
    /// document, and the revision makes a saved image attributable to a known state.
    pub(crate) fn document_identity(&self) -> Option<DocumentId> {
        self.session.active().map(|st| DocumentId::new(self.session.active_generation(), st.revision))
    }
}
