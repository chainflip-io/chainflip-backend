import assert from 'assert';
import { createStateChainKeypair } from 'shared/utils';
import { snowWhite, submitGovernanceExtrinsic } from 'shared/cf_governance';
import { getChainflipApi } from 'shared/utils/substrate';
import { signSendAndWait } from 'shared/utils/dedot';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { governanceExecutedEvent } from 'generated/events/governance/executed';
import type { PalletCfGovernanceCouncil } from 'generated/chaintypes/chainflip-node';
import { AccountId32 } from 'dedot/codecs';

// The governance members are a tree of groups, so collect the individuals at its leaves.
function flattenMembers(council: PalletCfGovernanceCouncil): string[] {
  switch (council.type) {
    case 'Individual':
      return [council.value.id.address(2112)];
    case 'SimpleGroup':
      return council.value.members.flatMap(flattenMembers);
    case 'WeightedGroup':
      return council.value.members.flatMap(([, member]) => flattenMembers(member));
    default:
      throw new Error(`unknown council node: ${JSON.stringify(council)}`);
  }
}

function individual(address: string): PalletCfGovernanceCouncil {
  return { type: 'Individual', value: { id: new AccountId32(address) } };
}

function simpleGroup(members: string[], threshold: number): PalletCfGovernanceCouncil {
  return { type: 'SimpleGroup', value: { threshold, members: members.map(individual) } };
}

async function getGovernanceMembers(): Promise<string[]> {
  await using chainflip = await getChainflipApi();

  return flattenMembers(await chainflip.query.governance.members());
}

const alice = createStateChainKeypair('//Alice');
const bob = createStateChainKeypair('//Bob');

// Snow White (weight 2) can't reach the threshold of 3 alone: the nested group (weight 1) has to
// approve too, which takes either one of Alice or Bob.
function nestedCouncil(): PalletCfGovernanceCouncil {
  return {
    type: 'WeightedGroup',
    value: {
      threshold: 3,
      members: [
        [2, individual(snowWhite.address)],
        [1, simpleGroup([alice.address, bob.address], 1)],
      ],
    },
  };
}

async function setNestedCouncil<A = []>(cf: ChainflipIO<A>, initMembers: string[]) {
  assert.strictEqual(
    initMembers.length,
    1,
    `Governance should have 1 member (Snow White), but found ${initMembers}`,
  );

  await cf.submitGovernance({
    extrinsic: (api) => api.tx.governance.setCouncil(nestedCouncil()),
  });

  cf.debug('Set the nested governance council!');
}

async function submitWithMultipleGovernanceMembers<A = []>(cf: ChainflipIO<A>) {
  // Ensure the nested council is in place before submitting the proposal
  const initMembers = await getGovernanceMembers();

  if (!initMembers.includes(alice.address)) {
    await setNestedCouncil(cf, initMembers);
  }

  const members = await getGovernanceMembers();

  cf.debug(`Current governance members: ${members}`);

  assert.deepStrictEqual(
    [...members].sort(),
    [snowWhite.address, alice.address, bob.address]
      .map((address) => new AccountId32(address).address(2112))
      .sort(),
    'Governance should consist of the nested council members',
  );

  // Killing 2 birds with 1 stone: testing governance execution with multiple
  // members *and* restoring governance to its original state
  const proposalId = await submitGovernanceExtrinsic(
    (chainflip) => chainflip.tx.governance.setCouncil(simpleGroup([snowWhite.address], 1)),
    cf.logger,
  );

  cf.info(`Submitted governance proposal with ID: ${proposalId}`);

  await using chainflip = await getChainflipApi();

  // Snow White's weight alone is below the threshold, so Alice approves for the nested group:
  await signSendAndWait(
    chainflip,
    chainflip.tx.governance.approve(proposalId),
    alice,
    alice.address,
  );
  cf.info(`Approved governance proposal with ID: ${proposalId}`);

  await cf.stepUntilEvent(governanceExecutedEvent.refine((id) => id === proposalId));

  assert.strictEqual(
    (await getGovernanceMembers()).length,
    1,
    'Governance should have been restored to 1 member',
  );

  cf.debug('Restored single member governance!');
}

export async function testMultipleMembersGovernance(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await submitWithMultipleGovernanceMembers(cf);
}
