//! A file service that answers from a script, for tests of the flows that ask for files. The
//! library has no scripted service, so this one implements [`Files`] itself.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use craft::host::{FileError, Files, PickId, PickRequest, Picked};

#[derive(Default)]
struct State {
    next: u64,
    asked: Vec<PickRequest>,
    /// Asked and not answered yet, newest last: dialogs the user is still in.
    open: Vec<(PickId, usize)>,
    answered: Vec<(PickId, Picked)>,
    /// Answers handed out as picks arrive (`None`: the user cancels). `None` as a whole: picks stay
    /// open until [`ScriptedFiles::answer`].
    script: Option<VecDeque<Option<Picked>>>,
}

/// Answers each pick with the next of a script, or keeps it open until the test answers it, and
/// records the requests.
#[derive(Default)]
pub struct ScriptedFiles(Mutex<State>);

impl ScriptedFiles {
    /// Picks stay open until [`Self::answer`].
    pub fn held() -> Arc<Self> {
        Arc::default()
    }

    /// Each pick is answered with the next of `answers` (`None`: Cancel; a spent script cancels).
    pub fn answering(answers: Vec<Option<Picked>>) -> Arc<Self> {
        let files = Self::default();
        files.state().script = Some(answers.into());
        Arc::new(files)
    }

    /// For `Services::files`.
    pub fn service(self: &Arc<Self>) -> Option<Arc<dyn Files>> {
        Some(Arc::clone(self) as Arc<dyn Files>)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every request asked so far, in order.
    pub fn requests(&self) -> Vec<PickRequest> {
        self.state().asked.clone()
    }

    /// How many picks the user is still in.
    pub fn open(&self) -> usize {
        self.state().open.len()
    }

    /// The user answers the newest open pick.
    pub fn answer(&self, picked: Picked) {
        let mut state = self.state();
        let (id, _) = state.open.pop().expect("a dialog is open");
        state.answered.push((id, picked));
    }
}

impl Files for ScriptedFiles {
    fn pick(&self, request: PickRequest) -> PickId {
        let mut state = self.state();
        state.next += 1;
        let id = PickId(state.next);
        state.asked.push(request);
        let at = state.asked.len() - 1;
        match state.script.as_mut() {
            Some(script) => {
                let picked = script.pop_front().flatten().unwrap_or(Picked::Cancelled);
                state.answered.push((id, picked));
            }
            None => state.open.push((id, at)),
        }
        id
    }

    fn take_answer(&self, id: PickId) -> Option<Picked> {
        let mut state = self.state();
        let at = state.answered.iter().position(|(i, _)| *i == id)?;
        Some(state.answered.remove(at).1)
    }

    fn forget(&self, id: PickId) {
        let mut state = self.state();
        state.answered.retain(|(i, _)| *i != id);
        state.open.retain(|(i, _)| *i != id);
    }

    fn pick_pending(&self) -> bool {
        !self.state().open.is_empty()
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>, FileError> {
        std::fs::read(path).map_err(FileError::from)
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), FileError> {
        std::fs::write(path, bytes).map_err(FileError::from)
    }

    fn last_folder(&self, _purpose: &str) -> Option<PathBuf> {
        None
    }
}
