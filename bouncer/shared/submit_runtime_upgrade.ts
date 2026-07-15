import { promises as fs } from 'fs';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { decodeDispatchError, sleep } from 'shared/utils';
import type { CfPrimitivesSemVer } from 'generated/chaintypes/chainflip-node';
import { tryRuntimeUpgrade } from 'shared/try_runtime_upgrade';
import { getChainflipApi, clearChainflipClientCache } from 'shared/utils/substrate';
import { throwError } from 'shared/utils/logger';
import { systemCodeUpdatedEvent } from 'generated/events/system/codeUpdated';
import { governanceFailedExecutionEvent } from 'generated/events/governance/failedExecution';
import { reputationPenaltyUpdatedEvent } from 'generated/events/reputation/penaltyUpdated';
import { ChainflipIO } from 'shared/utils/chainflip_io';

async function readRuntimeWasmFromFile(filePath: string): Promise<Uint8Array> {
  // Return the raw WASM bytes. The dedot codec length-prefixes the `code: Bytes` argument itself
  // when encoding the extrinsic, so we must NOT pre-prepend a compact length here like polkadot.js did.
  return new Uint8Array(await fs.readFile(filePath));
}

// By default we don't want to restrict that any of the nodes need to be upgraded.
export async function submitRuntimeUpgradeWithRestrictions<A = []>(
  cf: ChainflipIO<A>,
  wasmPath: string,
  semverRestriction?: Record<string, number>,
  percentNodesUpgraded = 0,
  // default to false, because try-runtime feature is not always available.
  tryRuntime = false,
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
      api.tx.reputation.setPenalty(
        { type: 'MissedAuthorshipSlot' },
        {
          reputation: 0,
          suspension: 0,
        },
      ),
    expectedEvent: reputationPenaltyUpdatedEvent,
  });

  cf.info(`Submitting runtime upgrade. WASM size is ${wasmStats.size} bytes.`);
  await submitGovernanceExtrinsic((api) =>
    api.tx.governance.chainflipRuntimeUpgrade(
      versionPercentRestriction as [CfPrimitivesSemVer, number] | undefined,
      runtimeWasm,
    ),
  );

  cf.info('Submitted runtime upgrade. Waiting for the runtime upgrade to complete.');
  const resultEvent = await cf.stepUntilOneEventOf({
    codeUpdated: systemCodeUpdatedEvent,
    failedExecution: governanceFailedExecutionEvent,
  });

  if (resultEvent.key === 'failedExecution') {
    const reason = decodeDispatchError(resultEvent.data, await getChainflipApi());
    throwError(cf.logger, new Error(`Runtime upgrade failed: ${reason}`));
  }

  // waiting for some time so that the new runtime version is reflected through the rpc layer
  await sleep(20000);

  cf.info('Runtime upgrade completed.');

  // The runtime upgrade changes the metadata (e.g. new pallets shift call indices). Drop the cached
  // dedot client so the next extrinsic is built and submitted against a fresh client that loads the
  // new runtime's metadata from scratch.
  await clearChainflipClientCache();

  cf.info('Restoring MissedAuthorshipSlot penalty defaults after runtime upgrade.');
  await cf.submitGovernance({
    extrinsic: (api) =>
      api.tx.reputation.setPenalty(
        { type: 'MissedAuthorshipSlot' },
        {
          reputation: 120,
          suspension: 150,
        },
      ),
    expectedEvent: reputationPenaltyUpdatedEvent,
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
