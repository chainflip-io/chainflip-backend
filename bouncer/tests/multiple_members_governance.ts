import assert from 'assert';
import { createStateChainKeypair, tryUntilSuccess } from 'shared/utils';
import { snowWhite, submitGovernanceExtrinsic } from 'shared/cf_governance';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Logger } from 'shared/utils/logger';
import { Codec } from '@polkadot/types/types';

async function getGovernanceMembers(): Promise<string[]> {
  await using chainflip = await getChainflipApi();

  const { members, threshold: _ } = (await chainflip.query.governance.members()) as unknown as {
    members: Codec;
    threshold: number;
  };
  return members.toPrimitive() as string[];
}

const alice = createStateChainKeypair('//Alice');

async function addAliceToGovernance(logger: Logger, initMembers: string[] = []) {
  logger.debug(`Adding Alice to governance: ${alice.address}`);

  const newMembers = [...initMembers, alice.address];

  assert.strictEqual(
    newMembers.length,
    2,
    `Governance should have 2 members (Snow White and Alice), but found ${newMembers}`,
  );

  const newThreshold = newMembers.length;

  await submitGovernanceExtrinsic(
    (chainflip) => chainflip.tx.governance.newMembershipSet(newMembers, newThreshold),
    logger,
  );

  await observeEvent(logger, 'governance:Executed').event;

  await tryUntilSuccess(
    async () => {
      const members = await getGovernanceMembers();
      return members.length === newMembers.length;
    },
    6000,
    4,
  );

  logger.debug('Added Alice to governance!');
}

async function submitWithMultipleGovernanceMembers(logger: Logger) {
  // Ensure Alice is in governance before submitting the proposal
  const initMembers = await getGovernanceMembers();

  if (!initMembers.includes(alice.address)) {
    await addAliceToGovernance(logger, initMembers);
  }

  const members = await getGovernanceMembers();

  logger.debug(`Current governance members: ${members}`);

  // Killing 2 birds with 1 stone: testing governance execution with multiple
  // members *and* restoring governance to its original state
  const proposalId = await submitGovernanceExtrinsic(
    (chainflip) => chainflip.tx.governance.newMembershipSet([snowWhite.address], 1),
    logger,
  );

  logger.info(`Submitted governance proposal with ID: ${proposalId}`);

  await using chainflip = await getChainflipApi();

  // Note that with two members, we need to approve with the other account:
  const nonce = (await chainflip.rpc.system.accountNextIndex(alice.address)) as unknown as number;
  await chainflip.tx.governance.approve(proposalId).signAndSend(alice, { nonce });
  logger.info(`Approved governance proposal with ID: ${proposalId}`);
  await observeEvent(logger, 'governance:Executed', {
    test: (event) => Number(event.data[0]) === proposalId,
  }).event;

  assert.strictEqual(
    (await getGovernanceMembers()).length,
    1,
    'Governance should have been restored to 1 member',
  );

  logger.debug('Removed Alice from governance!');
}

export async function testMultipleMembersGovernance(testContext: TestContext) {
  await submitWithMultipleGovernanceMembers(testContext.logger);
}
