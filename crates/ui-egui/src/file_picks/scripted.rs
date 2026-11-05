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
    /// A pick is a test failure: the flow under test must not show a picker.
    refuse: Option<String>,
    /// A Save is answered with the suggested name; every other pick is cancelled.
    save_suggested: bool,
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

    /// Every Save is answered with the suggested name (in the suggested folder); other picks are cancelled.
    pub fn saving_suggested() -> Option<Arc<dyn Files>> {
        let files = Self::default();
        files.state().save_suggested = true;
        Arc::new(files).service()
    }

    /// Any pick fails the test with `why`.
    pub fn refusing(why: &str) -> Option<Arc<dyn Files>> {
        let files = Self::default();
        files.state().refuse = Some(why.to_owned());
        Arc::new(files).service()
    }

    /// For `Services::files`.
    pub fn service(self: &Arc<Self>) -> Option<Arc<dyn Files>> {
        Some(Arc::clone(self) as Arc<dyn Files>)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The requests asked so far, in order. A service that holds its picks lists only the ones the
    /// user is still in.
    pub fn requests(&self) -> Vec<PickRequest> {
        let state = self.state();
        match state.script {
            Some(_) => state.asked.clone(),
            None => state.open.iter().map(|(_, at)| state.asked[*at].clone()).collect(),
        }
    }

    /// The kind of each request asked so far, in order.
    pub fn kinds(&self) -> Vec<craft::host::PickKind> {
        self.requests().iter().map(|r| r.kind).collect()
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
        if let Some(why) = &state.refuse {
            panic!("{why}");
        }
        state.next += 1;
        let id = PickId(state.next);
        let picked = (state.save_suggested).then(|| match request.kind {
            craft::host::PickKind::Save => {
                let name = PathBuf::from(request.file_name.clone().unwrap_or_default());
                Picked::Paths(vec![request.directory.as_ref().map_or(name.clone(), |d| d.join(&name))])
            }
            _ => Picked::Cancelled,
        });
        state.asked.push(request);
        if let Some(picked) = picked {
            state.answered.push((id, picked));
            return id;
        }
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

/// A pick answered with these paths.
pub fn paths<P: Into<PathBuf>>(paths: impl IntoIterator<Item = P>) -> Picked {
    Picked::Paths(paths.into_iter().map(Into::into).collect())
}

/// A pick answered with the path to save to.
pub fn save_to(path: impl Into<PathBuf>) -> Picked {
    Picked::Paths(vec![path.into()])
}

/// A pick answered with a folder.
pub fn folder(path: impl Into<PathBuf>) -> Picked {
    save_to(path)
}

/// A pick answered with one file's name and bytes, as the web's picker does.
pub fn contents(name: impl Into<String>, bytes: Vec<u8>) -> Picked {
    Picked::Contents(vec![(name.into(), bytes)])
}

/// A pick answered with several files' names and bytes.
pub fn contents_many(files: Vec<(String, Vec<u8>)>) -> Picked {
    Picked::Contents(files)
}

/// A service answering each pick with the next of `answers`, and the handle that lists the requests.
pub fn fake(answers: Vec<Option<Picked>>) -> (Option<Arc<dyn Files>>, Arc<ScriptedFiles>) {
    let files = ScriptedFiles::answering(answers);
    (files.service(), files)
}
