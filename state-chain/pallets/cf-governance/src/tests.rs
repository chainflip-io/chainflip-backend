use crate::{
	mock::*, ActiveProposals, Error, ExecutionMode, ExecutionPipeline, ExpiryTime, Members,
	PreAuthorisedGovCalls, ProposalIdCounter,
};
use cf_primitives::SemVer;
use cf_test_utilities::last_event;
use cf_traits::mocks::time_source;
use frame_support::{assert_err, assert_noop, assert_ok};
use sp_runtime::Percent;
use sp_std::collections::btree_set::BTreeSet;
use std::time::Duration;

use crate as pallet_cf_governance;

const DUMMY_WASM_BLOB: Vec<u8> = vec![];

fn mock_extrinsic() -> Box<RuntimeCall> {
	Box::new(RuntimeCall::Governance(pallet_cf_governance::Call::<Test>::new_membership_set {
		new_members: BTreeSet::from_iter([EVE, PETER, MAX]),
	}))
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
fn not_a_member() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(EVE),
				mock_extrinsic(),
				ExecutionMode::Automatic,
			),
			<Error<Test>>::NotMember
		);
	});
}

#[test]
fn propose_a_governance_extrinsic_and_expect_execution() {
	new_test_ext()
		.execute_with(|| {
			// Propose a governance extrinsic
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				mock_extrinsic(),
				ExecutionMode::Automatic,
			));
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
			);
			// Do the second approval to reach majority
			assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		})
		.then_execute_at_next_block(|_| {
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
			mock_extrinsic(),
			ExecutionMode::Automatic,
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
	const START_TIME: Duration = Duration::from_secs(10);
	const END_TIME: Duration = Duration::from_secs(7300);

	new_test_ext()
		.execute_with(|| {
			time_source::Mock::reset_to(START_TIME);
		})
		.then_execute_at_next_block(|_| {
			// Propose governance extrinsic
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				mock_extrinsic(),
				ExecutionMode::Automatic,
			));
		})
		.then_execute_at_next_block(|_| {
			// Set the time to be higher than the expiry time
			time_source::Mock::reset_to(END_TIME);
		})
		.then_execute_at_next_block(|_| {
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
			mock_extrinsic(),
			ExecutionMode::Automatic,
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
			mock_extrinsic(),
			ExecutionMode::Automatic,
		));
		assert_eq!(
			last_event::<Test>(),
			crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
		);
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(BOB),
			mock_extrinsic(),
			ExecutionMode::Automatic,
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
	new_test_ext()
		.execute_with(|| {
			// Define a sudo call
			let sudo_call = Box::new(RuntimeCall::System(
				frame_system::Call::<Test>::set_code_without_checks { code: vec![1, 2, 3, 4] },
			));
			// Wrap the sudo call as governance extrinsic
			let governance_extrinsic = Box::new(RuntimeCall::Governance(
				pallet_cf_governance::Call::<Test>::call_as_sudo { call: sudo_call },
			));
			// Propose the governance extrinsic
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				governance_extrinsic,
				ExecutionMode::Automatic,
			));
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(crate::Event::Approved(1)),
			);
			// Do the second necessary approval
			assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		})
		.then_execute_at_next_block(|_| {
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
			None,
			DUMMY_WASM_BLOB
		));
	});
}

#[test]
fn wrong_upgrade_conditions() {
	UpgradeConditionMock::set(false);
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::chainflip_runtime_upgrade(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				None,
				DUMMY_WASM_BLOB
			),
			<Error<Test>>::UpgradeConditionsNotMet
		);
	});
}

#[test]
fn error_during_runtime_upgrade() {
	RuntimeUpgradeMock::upgrade_success(false);
	UpgradeConditionMock::set(true);
	new_test_ext().execute_with(|| {
		// assert_noop! is not working when we emit an event and
		// the result is an error
		let result = Governance::chainflip_runtime_upgrade(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			None,
			DUMMY_WASM_BLOB,
		);
		assert!(result.is_err());
		assert_err!(result, frame_system::Error::<Test>::FailedToExtractRuntimeVersion);
	});
}

#[test]
fn runtime_upgrade_requires_up_to_date_authorities_cfes() {
	RuntimeUpgradeMock::upgrade_success(true);
	UpgradeConditionMock::set(true);
	const DESIRED_CFE_VERSION: SemVer = SemVer { major: 1, minor: 2, patch: 3 };
	new_test_ext().execute_with(|| {
		// This is how many nodes are *at* the required version.
		PercentCfeAtTargetVersion::set(Percent::from_percent(50));
		assert_ok!(Governance::chainflip_runtime_upgrade(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			Some((DESIRED_CFE_VERSION, Percent::from_percent(50))),
			DUMMY_WASM_BLOB,
		));

		assert_noop!(
			Governance::chainflip_runtime_upgrade(
				pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
				Some((DESIRED_CFE_VERSION, Percent::from_percent(51))),
				DUMMY_WASM_BLOB,
			),
			crate::Error::<Test>::NotEnoughAuthoritiesCfesAtTargetVersion
		);
	});
}

#[test]
fn runtime_upgrade_can_have_no_cfes_version_requirement() {
	RuntimeUpgradeMock::upgrade_success(true);
	UpgradeConditionMock::set(true);
	new_test_ext().execute_with(|| {
		PercentCfeAtTargetVersion::set(Percent::from_percent(0));

		assert_ok!(Governance::chainflip_runtime_upgrade(
			pallet_cf_governance::RawOrigin::GovernanceApproval.into(),
			None,
			DUMMY_WASM_BLOB,
		));
	});
}

#[test]
fn whitelisted_gov_call() {
	new_test_ext().execute_with(|| {
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic(),
			ExecutionMode::Manual,
		));
		assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		assert!(PreAuthorisedGovCalls::<Test>::contains_key(1));
		assert_ok!(Governance::dispatch_whitelisted_call(RuntimeOrigin::signed(CHARLES), 1));
		assert!(!PreAuthorisedGovCalls::<Test>::contains_key(1));
	});
}

#[test]
fn replacing_governance_members() {
	new_test_ext().execute_with(|| {
		assert_eq!(Members::<Test>::get(), BTreeSet::from_iter([ALICE, BOB, CHARLES]));
		assert_eq!(System::sufficients(&ALICE), 1);
		assert_eq!(System::sufficients(&BOB), 1);
		assert_eq!(System::sufficients(&CHARLES), 1);
		assert_eq!(System::sufficients(&EVE), 0);
		assert_eq!(System::sufficients(&PETER), 0);
		assert_eq!(System::sufficients(&MAX), 0);

		assert_ok!(Governance::new_membership_set(
			crate::RawOrigin::GovernanceApproval.into(),
			BTreeSet::from_iter([EVE, PETER, MAX])
		));

		assert_eq!(Members::<Test>::get(), BTreeSet::from_iter([EVE, PETER, MAX]));
		assert_eq!(System::sufficients(&ALICE), 0);
		assert_eq!(System::sufficients(&BOB), 0);
		assert_eq!(System::sufficients(&CHARLES), 0);
		assert_eq!(System::sufficients(&EVE), 1);
		assert_eq!(System::sufficients(&PETER), 1);
		assert_eq!(System::sufficients(&MAX), 1);

		assert_ok!(Governance::new_membership_set(
			crate::RawOrigin::GovernanceApproval.into(),
			BTreeSet::from_iter([ALICE, EVE, PETER])
		));
		assert_eq!(Members::<Test>::get(), BTreeSet::from_iter([ALICE, EVE, PETER]));
		assert_eq!(System::sufficients(&ALICE), 1);
		assert_eq!(System::sufficients(&BOB), 0);
		assert_eq!(System::sufficients(&CHARLES), 0);
		assert_eq!(System::sufficients(&EVE), 1);
		assert_eq!(System::sufficients(&PETER), 1);
		assert_eq!(System::sufficients(&MAX), 0);
	});
}
