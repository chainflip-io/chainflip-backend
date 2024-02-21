use super::*;
use frame_support::dispatch::GetDispatchInfo;
use pallet_cf_flip::FlipTransactionPayment;
use pallet_transaction_payment::OnChargeTransaction;

#[test]
// Governance is allowed to make free calls to governance gated extrinsics.
fn governance_members_pay_no_fees_for_governance_extrinsics() {
	super::genesis::with_test_defaults().build().execute_with(|| {
		let call: state_chain_runtime::RuntimeCall =
			frame_system::Call::remark { remark: vec![] }.into();
		let gov_call: state_chain_runtime::RuntimeCall =
			pallet_cf_governance::Call::approve { approved_id: 1 }.into();
		// Expect a successful normal call to work
		let ordinary = FlipTransactionPayment::<Runtime>::withdraw_fee(
			&ALICE.into(),
			&call,
			&call.get_dispatch_info(),
			5,
			0,
		);
		assert!(ordinary.expect("we have a result").is_some(), "expected Some(Surplus)");
		// Expect a successful gov call to work
		let gov = FlipTransactionPayment::<Runtime>::withdraw_fee(
			&ERIN.into(),
			&gov_call,
			&gov_call.get_dispatch_info(),
			5000,
			0,
		);
		assert!(gov.expect("we have a result").is_none(), "expected None");
		// Expect a non gov call to fail when it's executed by gov member
		let gov_err = FlipTransactionPayment::<Runtime>::withdraw_fee(
			&ERIN.into(),
			&call,
			&call.get_dispatch_info(),
			5000,
			0,
		);
		assert!(gov_err.is_err(), "expected an error");
	});
}
