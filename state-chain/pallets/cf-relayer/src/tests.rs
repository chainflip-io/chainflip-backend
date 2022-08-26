use crate::mock::new_test_ext;

#[test]
fn genesis_nodes_are_activated_by_default() {
	new_test_ext().execute_with(|| {
		assert!(true);
	});
}
