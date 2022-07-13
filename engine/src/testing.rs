#![cfg(test)]

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use futures::{Future, FutureExt};
use tempfile::{self, TempDir};
use utilities::assert_ok;

/// Checks that a given future yields without producing a result (yet) / is blocked by something
pub fn assert_future_awaits(f: impl Future) {
    assert!(f.now_or_never().is_none());
}

/// Checks if a given future either is ready, or will become ready on the next poll/without yielding
pub fn assert_future_can_complete<I>(f: impl Future<Output = I>) -> I {
    assert_ok!(f.now_or_never())
}

pub fn with_file<C: FnOnce(&Path)>(text: &[u8], closure: C) {
    let mut tempfile = tempfile::NamedTempFile::new().unwrap();
    tempfile.write_all(text).unwrap();
    closure(tempfile.path());
}

/// Create a temp directory that will be deleted when `TempDir` is dropped.
/// Also returns the path to a non-existent file in the directory.
pub fn new_temp_directory_with_nonexistent_file() -> (TempDir, PathBuf) {
    let tempdir = tempfile::TempDir::new().unwrap();
    let tempfile = tempdir.path().to_owned().join("file");
    assert!(!tempfile.exists());
    (tempdir, tempfile)
}

#[test]
fn test_stuff() {
    println!("Here's the file and line number: {} : {}", file!(), line!())
}
