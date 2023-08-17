#!/usr/bin/env -S pnpm tsx
import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { assert } from '@polkadot/util';
import { getChainflipApi, observeEvent, runWithTimeout } from '../shared/utils';
import { snowWhite, submitGovernanceExtrinsic } from '../shared/cf_governance';

async function getGovernanceMembers(): Promise<string[]> {
  const chainflip = await getChainflipApi();

  const res = (await chainflip.query.governance.members()).toJSON();
  return res as string[];
}

async function setGovernanceMembers(members: string[]) {
  const chainflip = await getChainflipApi();

  await submitGovernanceExtrinsic(chainflip.tx.governance.newMembershipSet(members), true);
}

await cryptoWaitReady();
const keyring = new Keyring({ type: 'sr25519' });
keyring.setSS58Format(2112);

const alice = keyring.createFromUri('//Alice');

async function addAliceToGovernance() {
  const initMembers = await getGovernanceMembers();
  assert(initMembers.length === 1, 'Governance should only have 1 member');

  const newMembers = [...initMembers, alice.address];

  await setGovernanceMembers(newMembers);

  const chainflip = await getChainflipApi();
  await observeEvent('governance:Executed', chainflip);

  assert((await getGovernanceMembers()).length === 2, 'Governance should now have 2 members');

  console.log('Added Alice to governance!');
}

async function submitWithMultipleGovernanceMembers() {
  const chainflip = await getChainflipApi();

  // Killing 2 birds with 1 stone: testing governance execution with multiple
  // members *and* restoring governance to its original state
  await submitGovernanceExtrinsic(chainflip.tx.governance.newMembershipSet([snowWhite.address]));

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

async function main() {
  console.log('=== Testing multiple members governance ===');
  await addAliceToGovernance();
  await submitWithMultipleGovernanceMembers();

  console.log('=== Multiple members governance test complete ===');
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
