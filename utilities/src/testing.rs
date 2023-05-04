use core::time::Duration;
use futures::{Future, FutureExt};
use std::{
	io::Write,
	path::{Path, PathBuf},
};
use tempfile::{self, TempDir};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::assert_ok;

pub mod logging;

const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

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

pub async fn recv_with_timeout<I>(receiver: &mut UnboundedReceiver<I>) -> Option<I> {
	tokio::time::timeout(CHANNEL_TIMEOUT, receiver.recv()).await.ok()?
}

pub async fn expect_recv_with_timeout<Item: std::fmt::Debug>(
	receiver: &mut UnboundedReceiver<Item>,
) -> Item {
	match recv_with_timeout(receiver).await {
		Some(i) => i,
		None => panic!("Timeout waiting for message, expected {}", std::any::type_name::<Item>()),
	}
}
