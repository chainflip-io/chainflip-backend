import assert from 'assert';
import Keyring from '../polkadot/keyring';
import { getChainflipApi, observeEvent } from '../shared/utils';
import { snowWhite, submitGovernanceExtrinsic } from '../shared/cf_governance';

async function getGovernanceMembers(): Promise<string[]> {
  await using chainflip = await getChainflipApi();

  const res = (await chainflip.query.governance.members()).toJSON();
  return res as string[];
}

async function setGovernanceMembers(members: string[]) {
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.governance.newMembershipSet(members));
}

const keyring = new Keyring({ type: 'sr25519' });
keyring.setSS58Format(2112);

const alice = keyring.createFromUri('//Alice');

async function addAliceToGovernance() {
  const initMembers = await getGovernanceMembers();
  assert(initMembers.length === 1, 'Governance should only have 1 member');

  const newMembers = [...initMembers, alice.address];

  await setGovernanceMembers(newMembers);

  await using chainflip = await getChainflipApi();
  await observeEvent('governance:Executed', chainflip);

  assert((await getGovernanceMembers()).length === 2, 'Governance should now have 2 members');

  console.log('Added Alice to governance!');
}

async function submitWithMultipleGovernanceMembers() {
  // Killing 2 birds with 1 stone: testing governance execution with multiple
  // members *and* restoring governance to its original state
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.governance.newMembershipSet([snowWhite.address]),
  );

  await using chainflip = await getChainflipApi();

  const proposalId = Number((await observeEvent('governance:Proposed', chainflip)).data);

  // Note that with two members, we need to approve with the other account:
  await chainflip.tx.governance.approve(proposalId).signAndSend(alice, { nonce: -1 });

  const executedProposalId = Number((await observeEvent('governance:Executed', chainflip)).data);
  assert(proposalId === executedProposalId, 'Proposal Ids should match');

  assert(
    (await getGovernanceMembers()).length === 1,
    'Governance should have been restored to 1 member',
  );

  console.log('Removed Alice from governance!');
}

export async function testMultipleMembersGovernance() {
  console.log('=== Testing multiple members governance ===');
  await addAliceToGovernance();
  await submitWithMultipleGovernanceMembers();

  console.log('=== Multiple members governance test complete ===');
}
