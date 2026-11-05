//! Answers for tests of the flows that ask for files, over the library's scripted file service
//! ([`ScriptedFiles`]): `Picked` values in the shapes the desktop and web pickers give, and the
//! service as the `Arc<dyn Files>` the app's services hold.

use std::path::PathBuf;
use std::sync::Arc;

pub use craft::host::ScriptedFiles;
use craft::host::{Files, PickKind, Picked};

/// A service whose picks stay open until the test answers them.
pub fn held() -> Arc<ScriptedFiles> {
    Arc::new(ScriptedFiles::held())
}

/// `files` as `Services::files`.
pub fn service(files: &Arc<ScriptedFiles>) -> Option<Arc<dyn Files>> {
    Some(Arc::clone(files) as Arc<dyn Files>)
}

/// The kind of each request asked so far, in order.
pub fn kinds(files: &ScriptedFiles) -> Vec<PickKind> {
    files.requests().iter().map(|r| r.kind).collect()
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

/// A service answering the nth pick with the nth of `answers` (`None`: Cancel), and the handle that
/// lists the requests.
pub fn fake(answers: Vec<Option<Picked>>) -> (Option<Arc<dyn Files>>, Arc<ScriptedFiles>) {
    let files = Arc::new(ScriptedFiles::answering(answers.into_iter().map(|a| a.unwrap_or(Picked::Cancelled))));
    (service(&files), files)
}
