import { compactAddLength } from '@polkadot/util';
import { promises as fs } from 'fs';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { decodeDispatchError } from 'shared/utils';
import { tryRuntimeUpgrade } from 'shared/try_runtime_upgrade';
import { getChainflipApi } from 'shared/utils/substrate';
import { throwError } from 'shared/utils/logger';
import { systemCodeUpdated } from 'generated/events/system/codeUpdated';
import { governanceFailedExecution } from 'generated/events/governance/failedExecution';
import { ChainflipIO } from 'shared/utils/chainflip_io';

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

  cf.info(`Submitting runtime upgrade. WASM size is ${wasmStats.size} bytes.`);
  await submitGovernanceExtrinsic((api) =>
    api.tx.governance.chainflipRuntimeUpgrade(versionPercentRestriction, runtimeWasm),
  );

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

  cf.info('Runtime upgrade completed.');
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
