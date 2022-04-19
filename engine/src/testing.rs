#![cfg(test)]

use std::{io::Write, path::Path};

use futures::{Future, FutureExt};
use tempfile;

/// Simply unwraps the value. Advantage of this is to make it clear in tests
/// what we are testing
macro_rules! assert_ok {
    ($result:expr) => {
        $result.unwrap()
    };
}
pub(crate) use assert_ok;

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

mod tests {
    #[test]
    fn test_assert_ok_unwrap_ok() {
        fn works() -> Result<i32, i32> {
            Ok(1)
        }
        let result = assert_ok!(works());
        assert_eq!(result, 1);
    }

    #[test]
    #[should_panic]
    fn test_assert_ok_err() {
        fn works() -> Result<i32, i32> {
            Err(0)
        }
        assert_ok!(works());
    }
}

#[test]
fn test_stuff() {
    println!("Here's the file and line number: {} : {}", file!(), line!())
}
