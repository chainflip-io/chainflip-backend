// eslint-disable-next-line no-restricted-imports -- type-only import; runtime init is handled elsewhere via `polkadot/keyring`.
import type { KeyringPair } from '@polkadot/keyring/types';
import { compactAddLength } from '@polkadot/util';
import { promises as fs } from 'fs';
import { submitGovernanceApproval, submitGovernanceExtrinsic } from 'shared/cf_governance';
import { decodeDispatchError, sleep } from 'shared/utils';
import { tryRuntimeUpgrade } from 'shared/try_runtime_upgrade';
import { getChainflipApi } from 'shared/utils/substrate';
import { throwError } from 'shared/utils/logger';
import { systemCodeUpdated } from 'generated/events/system/codeUpdated';
import { governanceFailedExecution } from 'generated/events/governance/failedExecution';
import { ChainflipIO } from 'shared/utils/chainflip_io';

// Multi-member council flow: submit, wait `leadTimeMs`, then `account` signs
// the second approval. Caller must have already arranged for threshold ≥ 2.
export type SecondApprover = {
  account: KeyringPair;
  leadTimeMs: number;
};

async function readRuntimeWasmFromFile(filePath: string): Promise<Uint8Array> {
  return compactAddLength(new Uint8Array(await fs.readFile(filePath)));
}

// By default we don't want to restrict that any of the nodes need to be upgraded.
export async function submitRuntimeUpgradeWithRestrictions<A = []>(
  cf: ChainflipIO<A>,
  wasmPath: string,
  semverRestriction?: Record<string, number>,
  percentNodesUpgraded = 0,
  // default to false, because try-runtime feature is not always available.
  tryRuntime = false,
  secondApprover?: SecondApprover,
) {
  const wasmStats = await fs.stat(wasmPath);
  const runtimeWasm = await readRuntimeWasmFromFile(wasmPath);

  const networkUrl = process.env.CF_NODE_ENDPOINT ?? 'ws://localhost:9944';

  if (tryRuntime) {
    cf.info('Running try-runtime before submitting the runtime upgrade.');
    await tryRuntimeUpgrade('last-n', networkUrl, wasmPath);
  }

  let versionPercentRestriction;
  if (semverRestriction && percentNodesUpgraded) {
    versionPercentRestriction = [semverRestriction, percentNodesUpgraded];
  } else {
    versionPercentRestriction = undefined;
  }

  cf.info('Temporarily disabling MissedAuthorshipSlot punishment during runtime upgrade.');
  await cf.submitGovernance({
    extrinsic: (api) =>
      api.tx.reputation.setPenalty('MissedAuthorshipSlot', {
        reputation: 0,
        suspension: 0,
      }),
    expectedEvent: { name: 'Reputation.PenaltyUpdated' },
  });

  cf.info(`Submitting runtime upgrade. WASM size is ${wasmStats.size} bytes.`);
  const proposalId = await submitGovernanceExtrinsic((api) =>
    api.tx.governance.chainflipRuntimeUpgrade(versionPercentRestriction, runtimeWasm),
  );

  if (secondApprover) {
    cf.info(
      `Proposal ${proposalId}: waiting ${secondApprover.leadTimeMs}ms before second approval.`,
    );
    await sleep(secondApprover.leadTimeMs);
    await submitGovernanceApproval(proposalId, secondApprover.account, cf.logger);
  }

  cf.info('Submitted runtime upgrade. Waiting for the runtime upgrade to complete.');
  const resultEvent = await cf.stepUntilOneEventOf({
    codeUpdated: {
      name: 'System.CodeUpdated',
      schema: systemCodeUpdated,
    },
    failedExecution: {
      name: 'Governance.FailedExecution',
      schema: governanceFailedExecution,
    },
  });

  if (resultEvent.key === 'failedExecution') {
    const reason = decodeDispatchError(resultEvent.data, await getChainflipApi());
    throwError(cf.logger, new Error(`Runtime upgrade failed: ${reason}`));
  }

  // waiting for some time so that the new runtime version is reflected through the rpc layer
  await sleep(20000);

  cf.info('Runtime upgrade completed.');

  cf.info('Restoring MissedAuthorshipSlot penalty defaults after runtime upgrade.');
  await cf.submitGovernance({
    extrinsic: (api) =>
      api.tx.reputation.setPenalty('MissedAuthorshipSlot', {
        reputation: 120,
        suspension: 150,
      }),
    expectedEvent: { name: 'Reputation.PenaltyUpdated' },
  });
}

export async function submitRuntimeUpgradeWasmPath<A = []>(cf: ChainflipIO<A>, wasmPath: string) {
  await submitRuntimeUpgradeWithRestrictions(cf, wasmPath);
}

// Restrictions not provided.
export async function submitRuntimeUpgrade<A = []>(
  cf: ChainflipIO<A>,
  projectRoot: string,
  tryRuntime = false,
) {
  await submitRuntimeUpgradeWithRestrictions(
    cf,
    `${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
    undefined,
    0,
    tryRuntime,
  );
}
