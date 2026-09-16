// Copyright 2025 Chainflip Labs GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

use crate::{
	council::{tests::pro_3132_council, MAX_MEMBERS},
	mock::*,
	ActiveProposals, Council, Error, Event, ExecutionMode, ExecutionPipeline, ExpiryTime, Members,
	PendingMembers, PreAuthorisedGovCalls, ProposalIdCounter, Proposals,
};
use cf_primitives::SemVer;
use cf_test_utilities::last_event;
use cf_traits::mocks::time_source;
use frame_support::{assert_err, assert_noop, assert_ok};
use sp_runtime::{BuildStorage, Percent};
use sp_std::collections::btree_set::BTreeSet;
use std::time::Duration;

use crate as pallet_cf_governance;

const DUMMY_WASM_BLOB: Vec<u8> = vec![];

fn mock_extrinsic() -> Box<RuntimeCall> {
	Box::new(RuntimeCall::Governance(pallet_cf_governance::Call::<Test>::set_council {
		new_council: Council::simple_group(2, [EVE, PETER, MAX]),
	}))
}

/// Changes the threshold without adding members, so no incoming member has to approve it.
fn threshold_change_extrinsic() -> Box<RuntimeCall> {
	Box::new(RuntimeCall::Governance(pallet_cf_governance::Call::<Test>::set_council {
		new_council: Council::simple_group(1, [ALICE, BOB, CHARLES]),
	}))
}

#[test]
fn genesis_config() {
	new_test_ext().execute_with(|| {
		let genesis_members = Members::<Test>::get().members();
		assert!(genesis_members.contains(&ALICE));
		assert!(genesis_members.contains(&BOB));
		assert!(genesis_members.contains(&CHARLES));
		assert_eq!(Members::<Test>::get(), Council::simple_group(2, [ALICE, BOB, CHARLES]));
		let expiry_span = ExpiryTime::<Test>::get();
		assert_eq!(expiry_span, 50);
	});
}

#[test]
#[should_panic(expected = "TooManyMembers")]
fn genesis_rejects_invalid_council() {
	let _ = RuntimeGenesisConfig {
		system: Default::default(),
		governance: GovernanceConfig {
			members: (0..=MAX_MEMBERS as u64).collect(),
			expiry_span: 50,
		},
	}
	.build_storage();
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
			// Make sure only a governance member can propose
			assert_noop!(
				Governance::propose_governance_extrinsic(
					RuntimeOrigin::signed(NOT_GOV_MEMBER),
					threshold_change_extrinsic(),
					ExecutionMode::Automatic,
				),
				Error::<Test>::NotMember
			);
			// Propose a governance extrinsic
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				threshold_change_extrinsic(),
				ExecutionMode::Automatic,
			));
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(Event::Approved(1)),
			);
			// Make sure only a governance member can approve
			assert_noop!(
				Governance::approve(RuntimeOrigin::signed(NOT_GOV_MEMBER), 1),
				Error::<Test>::NotMember
			);
			// Do the second approval to reach majority
			assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		})
		.then_execute_at_next_block(|_| {
			// Expect the Executed event was fired
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(Event::Executed(1)),
			);
			// Check the proposal took effect
			assert_eq!(Members::<Test>::get(), Council::simple_group(1, [ALICE, BOB, CHARLES]));
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
			threshold_change_extrinsic(),
			ExecutionMode::Automatic,
		));
		// Assert the proposed event was fired
		assert_eq!(last_event::<Test>(), crate::mock::RuntimeEvent::Governance(Event::Approved(1)),);
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
				crate::mock::RuntimeEvent::Governance(Event::Expired(1)),
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
		assert_eq!(last_event::<Test>(), crate::mock::RuntimeEvent::Governance(Event::Approved(1)),);
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(BOB),
			mock_extrinsic(),
			ExecutionMode::Automatic,
		));
		assert_eq!(last_event::<Test>(), crate::mock::RuntimeEvent::Governance(Event::Approved(2)),);
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
			// Make sure that only governance can sudo, not governance member.
			assert_noop!(
				Governance::call_as_sudo(RuntimeOrigin::signed(ALICE), sudo_call.clone()),
				sp_runtime::traits::BadOrigin
			);
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
				crate::mock::RuntimeEvent::Governance(Event::Approved(1)),
			);
			// Do the second necessary approval
			assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		})
		.then_execute_at_next_block(|_| {
			// Expect the sudo extrinsic to be executed successfully
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(Event::Executed(1)),
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
fn only_governance_can_runtime_upgrade() {
	new_test_ext().execute_with(|| {
		assert_noop!(
			Governance::chainflip_runtime_upgrade(
				RuntimeOrigin::signed(ALICE),
				None,
				DUMMY_WASM_BLOB
			),
			sp_runtime::traits::BadOrigin
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
			threshold_change_extrinsic(),
			ExecutionMode::Manual,
		));
		assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		assert!(PreAuthorisedGovCalls::<Test>::contains_key(1));
		assert_noop!(
			Governance::dispatch_whitelisted_call(RuntimeOrigin::none(), 1),
			sp_runtime::traits::BadOrigin
		);
		assert_ok!(Governance::dispatch_whitelisted_call(RuntimeOrigin::signed(CHARLES), 1));
		assert!(!PreAuthorisedGovCalls::<Test>::contains_key(1));
	});
}

#[test]
fn replacing_governance_members() {
	new_test_ext().execute_with(|| {
		assert_eq!(Members::<Test>::get().members(), BTreeSet::from_iter([ALICE, BOB, CHARLES]));
		assert_eq!(System::sufficients(&ALICE), 1);
		assert_eq!(System::sufficients(&BOB), 1);
		assert_eq!(System::sufficients(&CHARLES), 1);
		assert_eq!(System::sufficients(&EVE), 0);
		assert_eq!(System::sufficients(&PETER), 0);
		assert_eq!(System::sufficients(&MAX), 0);

		// Make sure only governance can replace the members
		assert_noop!(
			Governance::set_council(
				RuntimeOrigin::signed(ALICE),
				Council::simple_group(2, [EVE, PETER, MAX]),
			),
			sp_runtime::traits::BadOrigin
		);
		assert_noop!(
			Governance::set_council(
				crate::RawOrigin::GovernanceApproval.into(),
				Council::simple_group(4, [EVE, PETER, MAX]),
			),
			Error::<Test>::UnreachableCouncilThreshold
		);
		assert_noop!(
			Governance::set_council(
				crate::RawOrigin::GovernanceApproval.into(),
				Council::SimpleGroup { threshold: 1, members: vec![] },
			),
			Error::<Test>::EmptyCouncilGroup
		);

		assert_ok!(Governance::set_council(
			crate::RawOrigin::GovernanceApproval.into(),
			Council::simple_group(2, [EVE, PETER, MAX]),
		));

		assert_eq!(Members::<Test>::get().members(), BTreeSet::from_iter([EVE, PETER, MAX]));
		assert_eq!(System::sufficients(&ALICE), 0);
		assert_eq!(System::sufficients(&BOB), 0);
		assert_eq!(System::sufficients(&CHARLES), 0);
		assert_eq!(System::sufficients(&EVE), 1);
		assert_eq!(System::sufficients(&PETER), 1);
		assert_eq!(System::sufficients(&MAX), 1);

		assert_ok!(Governance::set_council(
			crate::RawOrigin::GovernanceApproval.into(),
			Council::simple_group(2, [ALICE, EVE, PETER]),
		));
		assert_eq!(Members::<Test>::get().members(), BTreeSet::from_iter([ALICE, EVE, PETER]));
		assert_eq!(System::sufficients(&ALICE), 1);
		assert_eq!(System::sufficients(&BOB), 0);
		assert_eq!(System::sufficients(&CHARLES), 0);
		assert_eq!(System::sufficients(&EVE), 1);
		assert_eq!(System::sufficients(&PETER), 1);
		assert_eq!(System::sufficients(&MAX), 0);
	});
}

#[test]
fn weighted_council_passes_proposal_across_groups() {
	new_test_ext()
		.execute_with(|| {
			assert_ok!(Governance::set_council(
				crate::RawOrigin::GovernanceApproval.into(),
				pro_3132_council(),
			));
			// Member 1 of group 1 proposes.
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(101),
				Box::new(RuntimeCall::Governance(
					pallet_cf_governance::Call::<Test>::set_council {
						new_council: Council::simple_group(1, [101]),
					},
				)),
				ExecutionMode::Automatic,
			));
			// Individual 1 brings support to 20 of 50.
			assert_ok!(Governance::approve(RuntimeOrigin::signed(301), 1));
			// Group 1 is still short of its internal 3-of-7.
			assert_ok!(Governance::approve(RuntimeOrigin::signed(102), 1));
			assert!(ExecutionPipeline::<Test>::get().is_empty());
			// Group 1 passes internally, adding its 30 to reach the threshold of 50.
			assert_ok!(Governance::approve(RuntimeOrigin::signed(103), 1));
			assert_eq!(ExecutionPipeline::<Test>::decode_len(), Some(1));
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(Event::Executed(1)),
			);
		});
}

#[test]
fn nested_members_are_sufficient() {
	new_test_ext().execute_with(|| {
		assert_ok!(Governance::set_council(
			crate::RawOrigin::GovernanceApproval.into(),
			pro_3132_council(),
		));
		for member in pro_3132_council().members() {
			assert_eq!(System::sufficients(&member), 1);
		}
		for member in [ALICE, BOB, CHARLES] {
			assert_eq!(System::sufficients(&member), 0);
		}

		assert_ok!(Governance::set_council(
			crate::RawOrigin::GovernanceApproval.into(),
			Council::simple_group(1, [ALICE, 101]),
		));
		assert_eq!(System::sufficients(&ALICE), 1);
		assert_eq!(System::sufficients(&101), 1);
		assert_eq!(System::sufficients(&102), 0);
		assert_eq!(System::sufficients(&301), 0);
	});
}

#[test]
fn new_members_must_approve_their_own_inclusion() {
	new_test_ext()
		.execute_with(|| {
			// ALICE proposes (and implicitly approves) a new council of EVE, PETER and MAX.
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				mock_extrinsic(),
				ExecutionMode::Automatic,
			));
			assert_eq!(
				PendingMembers::<Test>::get(1),
				BTreeSet::from_iter([EVE, PETER, MAX]),
				"the incoming members are pending until they approve"
			);

			// Quorum of the *current* council is reached, but the incoming members have not
			// approved, so the proposal stays open.
			assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
			assert!(Proposals::<Test>::contains_key(1));
			assert!(ExecutionPipeline::<Test>::get().is_empty());

			// Two of the three incoming members are not enough either.
			assert_ok!(Governance::approve(RuntimeOrigin::signed(EVE), 1));
			assert_ok!(Governance::approve(RuntimeOrigin::signed(PETER), 1));
			assert!(Proposals::<Test>::contains_key(1));

			// The last incoming member unblocks it.
			assert_ok!(Governance::approve(RuntimeOrigin::signed(MAX), 1));
			assert_eq!(ExecutionPipeline::<Test>::decode_len().unwrap(), 1);
			assert!(!PendingMembers::<Test>::contains_key(1));
		})
		.then_execute_at_next_block(|_| {
			assert_eq!(Members::<Test>::get(), Council::simple_group(2, [EVE, PETER, MAX]));
		});
}

#[test]
fn incoming_approvals_do_not_count_towards_quorum() {
	new_test_ext().execute_with(|| {
		// Only ALICE has approved, so the old council is one approval short of its 2-of-3.
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic(),
			ExecutionMode::Automatic,
		));
		for incoming in [EVE, PETER, MAX] {
			assert_ok!(Governance::approve(RuntimeOrigin::signed(incoming), 1));
		}
		assert!(
			Proposals::<Test>::contains_key(1),
			"incoming members must not be able to pass a proposal on their own"
		);

		assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
		assert_eq!(ExecutionPipeline::<Test>::decode_len().unwrap(), 1);
	});
}

#[test]
fn incoming_members_can_only_approve_their_own_proposal() {
	new_test_ext().execute_with(|| {
		// Proposal 1 adds EVE, PETER and MAX; proposal 2 does not.
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic(),
			ExecutionMode::Automatic,
		));
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			Box::new(RuntimeCall::Governance(pallet_cf_governance::Call::<Test>::set_council {
				new_council: Council::simple_group(1, [CHARLES]),
			})),
			ExecutionMode::Automatic,
		));

		assert_noop!(Governance::approve(RuntimeOrigin::signed(EVE), 2), Error::<Test>::NotMember);
		assert_noop!(
			Governance::approve(RuntimeOrigin::signed(NOT_GOV_MEMBER), 1),
			Error::<Test>::NotMember
		);
	});
}

#[test]
fn incoming_members_get_an_account_to_sign_with() {
	const START_TIME: Duration = Duration::from_secs(10);
	const END_TIME: Duration = Duration::from_secs(7300);

	new_test_ext()
		.execute_with(|| {
			time_source::Mock::reset_to(START_TIME);
			assert_eq!(System::sufficients(&EVE), 0);

			// An incoming member needs an account before they can submit their approval:
			// `CheckNonce` rejects an extrinsic from an account with no providers or sufficients.
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				mock_extrinsic(),
				ExecutionMode::Automatic,
			));
			assert_eq!(System::sufficients(&EVE), 1);
		})
		.then_execute_at_next_block(|_| {
			time_source::Mock::reset_to(END_TIME);
		})
		.then_execute_at_next_block(|_| {
			// The proposal expired, so the account reference goes away again.
			assert_eq!(
				last_event::<Test>(),
				crate::mock::RuntimeEvent::Governance(Event::Expired(1))
			);
			assert_eq!(System::sufficients(&EVE), 0);
			assert!(!PendingMembers::<Test>::contains_key(1));
		});
}

#[test]
fn an_executed_proposal_leaves_one_reference_per_member() {
	new_test_ext()
		.execute_with(|| {
			assert_ok!(Governance::propose_governance_extrinsic(
				RuntimeOrigin::signed(ALICE),
				mock_extrinsic(),
				ExecutionMode::Automatic,
			));
			assert_ok!(Governance::approve(RuntimeOrigin::signed(BOB), 1));
			for incoming in [EVE, PETER, MAX] {
				assert_ok!(Governance::approve(RuntimeOrigin::signed(incoming), 1));
			}
		})
		.then_execute_at_next_block(|_| {
			// One reference from being installed as a member, not two: the pending reference
			// taken at proposal time is released when the proposal resolves.
			for member in [EVE, PETER, MAX] {
				assert_eq!(System::sufficients(&member), 1);
			}
			for former in [ALICE, BOB, CHARLES] {
				assert_eq!(System::sufficients(&former), 0);
			}
		});
}

#[test]
fn replacing_the_council_releases_pending_members() {
	new_test_ext().execute_with(|| {
		assert_ok!(Governance::propose_governance_extrinsic(
			RuntimeOrigin::signed(ALICE),
			mock_extrinsic(),
			ExecutionMode::Automatic,
		));
		assert_eq!(System::sufficients(&EVE), 1);

		// Replacing the council expires the open proposal, which must also release the
		// references it took for its incoming members.
		assert_ok!(Governance::set_council(
			crate::RawOrigin::GovernanceApproval.into(),
			Council::simple_group(1, [CHARLES]),
		));
		assert_eq!(System::sufficients(&EVE), 0);
		assert!(!PendingMembers::<Test>::contains_key(1));
	});
}
