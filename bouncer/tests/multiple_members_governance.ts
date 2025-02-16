import assert from 'assert';
import { createStateChainKeypair, tryUntilSuccess } from '../shared/utils';
import { snowWhite, submitGovernanceExtrinsic } from '../shared/cf_governance';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { TestContext } from '../shared/utils/test_context';
import { Logger } from '../shared/utils/logger';

async function getGovernanceMembers(): Promise<string[]> {
  await using chainflip = await getChainflipApi();

  const res = (await chainflip.query.governance.members()).toJSON();
  return res as string[];
}

async function setGovernanceMembers(members: string[]) {
  await submitGovernanceExtrinsic((chainflip) => chainflip.tx.governance.newMembershipSet(members));
}

const alice = createStateChainKeypair('//Alice');

async function addAliceToGovernance(logger: Logger) {
  const initMembers = await getGovernanceMembers();
  if (initMembers.includes(alice.address)) {
    logger.warn('Alice is already in governance!');
    return;
  }

  assert(initMembers.length === 1, 'Governance should only have 1 member');

  const newMembers = [...initMembers, alice.address];

  await setGovernanceMembers(newMembers);

  await observeEvent('governance:Executed').event;

  await tryUntilSuccess(async () => (await getGovernanceMembers()).length === 2, 3000, 10);

  logger.debug('Added Alice to governance!');
}

async function submitWithMultipleGovernanceMembers(logger: Logger) {
  // Killing 2 birds with 1 stone: testing governance execution with multiple
  // members *and* restoring governance to its original state
  await submitGovernanceExtrinsic((chainflip) =>
    chainflip.tx.governance.newMembershipSet([snowWhite.address]),
  );

  await using chainflip = await getChainflipApi();

  const proposalId = Number((await observeEvent('governance:Proposed').event).data);

  // Note that with two members, we need to approve with the other account:
  await chainflip.tx.governance.approve(proposalId).signAndSend(alice, { nonce: -1 });

  const executedProposalId = Number((await observeEvent('governance:Executed').event).data);
  assert(proposalId === executedProposalId, 'Proposal Ids should match');

  assert(
    (await getGovernanceMembers()).length === 1,
    'Governance should have been restored to 1 member',
  );

  logger.debug('Removed Alice from governance!');
}

export async function testMultipleMembersGovernance(testContext: TestContext) {
  await addAliceToGovernance(testContext.logger);
  await submitWithMultipleGovernanceMembers(testContext.logger);
}
