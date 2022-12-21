use crate::{
	mock::*, ActiveProposals, Error, ExecutionPipeline, ExpiryTime, Members, ProposalIdCounter,
};
use cf_test_utilities::last_event;
use cf_traits::mocks::time_source;
use frame_support::{assert_err, assert_noop, assert_ok, traits::OnInitialize};
use std::time::Duration;

use crate as pallet_cf_governance;

const DUMMY_WASM_BLOB: Vec<u8> = vec![];

fn mock_extrinsic() -> Box<Call> {
	Box::new(RuntimeCall::Governance(pallet_cf_governance::Call::<Test>::new_membership_set {
		accounts: vec![EVE, PETER, MAX],
	}))
}

fn next_block() {
	System::set_block_number(System::block_number() + 1);
	<Governance as OnInitialize<u64>>::on_initialize(System::block_number());
}

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		let genesis_members = Members::<Test>::get();
		assert!(genesis_members.contains(&ALICE));
		assert!(genesis_members.contains(&BOB));
		assert!(genesis_members.contains(&CHARLES));
		let expiry_span = ExpiryTime::<Test>::get();
		assert_eq!(expiry_span, 50);
	});
}

#[test]
fn governance_restriction() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::new_membership_set(RuntimeOrigin::signed(ALICE), vec![EVE, PETER, MAX]),
			frame_support::error::BadOrigin
		);
	});
}

#[test]
fn not_a_member() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::propose_governance_extrinsic(RuntimeOrigin::signed(EVE), mock_extrinsic()),
			<Error<Test>>::NotMember
		);
	});
}

#[test]
fn propose_a_governance_extrinsic_and_expect_execution() {
	new_test_ext().execute_with(|| {
		// Propose a governance extrinsic
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic()
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
		);
		// Do the second approval to reach majority
		assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		next_block();
		// Expect the Executed event was fired
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Executed(1)),
		);
		// Check the new governance set
		let genesis_members = Members::<Test>::get();
		assert!(genesis_members.contains(&EVE));
		assert!(genesis_members.contains(&PETER));
		// Check if the storage was cleaned up
		assert_eq!(ActiveProposals::<Test>::get().len(), 0);
		assert_eq!(ExecutionPipeline::<Test>::get().len(), 0);
	});
}

#[test]
fn already_executed() {
	new_test_ext().execute_with(|| {
		// Propose a governance extrinsic
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic()
		));
		// Assert the proposed event was fired
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
		);
		// Do the second approval to reach majority
		assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		// The third attempt in this block has to fail because the
		// proposal is already in the execution pipeline
		assert_noop!(
			Governance::approve(RuntimeOrigin::signed(ALICE), 1),
			<Error<Test>>::ProposalNotFound
		);
		assert_eq!(ExecutionPipeline::<Test>::decode_len().unwrap(), 1);
	});
}

#[test]
fn proposal_not_found() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::approve(RuntimeOrigin::signed(ALICE), 200),
			<Error<Test>>::ProposalNotFound
		);
	});
}

#[test]
fn propose_a_governance_extrinsic_and_expect_it_to_expire() {
	new_test_ext().execute_with(|| {
		const START_TIME: Duration = Duration::from_secs(10);
		const END_TIME: Duration = Duration::from_secs(7300);
		time_source::Mock::reset_to(START_TIME);
		next_block();
		// Propose governance extrinsic
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic()
		));
		next_block();
		// Set the time to be higher than the expiry time
		time_source::Mock::reset_to(END_TIME);
		next_block();
		// Expect the Expired event to be fired
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Expired(1)),
		);
		assert_eq!(ActiveProposals::<Test>::get().len(), 0);
	});
}

#[test]
fn can_not_vote_twice() {
	new_test_ext().execute_with(|| {
		// Propose a governance extrinsic
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic()
		));
		// Try to approve it again. Proposing implies approving.
		assert_noop!(
			Governance::approve(RuntimeOrigin::signed(ALICE), 1),
			<Error<Test>>::AlreadyApproved
		);
	});
}

#[test]
fn several_open_proposals() {
	new_test_ext().execute_with(|| {
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic()
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
		);
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(BOB),
			mock_extrinsic()
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Approved(2)),
		);
		assert_eq!(ProposalIdCounter::<Test>::get(), 2);
	});
}

#[test]
fn sudo_extrinsic() {
	new_test_ext().execute_with(|| {
		// Define a sudo call
		let sudo_call =
			Box::new(RuntimeCall::System(frame_system::Call::<Test>::set_code_without_checks {
				code: vec![1, 2, 3, 4],
			}));
		// Wrap the sudo call as governance extrinsic
		let governance_extrinsic =
			Box::new(RuntimeCall::Governance(pallet_cf_governance::Call::<Test>::call_as_sudo {
				call: sudo_call,
			}));
		// Propose the governance extrinsic
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			governance_extrinsic
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
		);
		// Do the second necessary approval
		assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		next_block();
		// Expect the sudo extrinsic to be executed successfully
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Executed(1)),
		);
	});
}

#[test]
fn upgrade_runtime_successfully() {
	new_test_ext().execute_with(|| {
		assert_ok!(Governance::chainflip_runtime_upgrade(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			DUMMY_WASM_BLOB
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::UpgradeConditionsSatisfied),
		);
	});
}

#[test]
fn wrong_upgrade_conditions() {
	UpgradeConditionMock::set(false);
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::chainflip_runtime_upgrade(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				DUMMY_WASM_BLOB
			),
			<Error<Test>>::UpgradeConditionsNotMet
		);
	});
}

#[test]
fn error_during_runtime_upgrade() {
	RuntimeUpgradeMock::set(false);
	UpgradeConditionMock::set(true);
	new_test_ext().execute_with(|| {
		// assert_noop! is not working when we emit an event and
		// the result is an error
		let result = Governance::chainflip_runtime_upgrade(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			DUMMY_WASM_BLOB,
		);
		assert!(result.is_err());
		assert_err!(result, frame_system::Error::<Test>::FailedToExtractRuntimeVersion);
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::UpgradeConditionsSatisfied),
		);
	});
}
