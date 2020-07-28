/// Just a sample integration test (testing across modules)
/// until we have real integration tests
#[test]
fn integration_test_add() {
    assert_eq!(blockswap::add(2, 5), 7);
}
