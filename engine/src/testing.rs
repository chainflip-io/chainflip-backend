#![cfg(test)]

/// Simply unwraps the value. Advantage of this is to make it clear in tests
/// what we are testing
macro_rules! assert_ok {
    ($result:expr) => {
        $result.unwrap()
    };
}
pub(crate) use assert_ok;

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
