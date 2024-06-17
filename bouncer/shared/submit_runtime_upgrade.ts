import { compactAddLength } from '@polkadot/util';
import { promises as fs } from 'fs';
import { submitGovernanceExtrinsic } from './cf_governance';
import { decodeModuleError } from '../shared/utils';
import { tryRuntimeUpgrade } from './try_runtime_upgrade';
import { getChainflipApi, observeEvent } from './utils/substrate';

async function readRuntimeWasmFromFile(filePath: string): Promise<Uint8Array> {
  return compactAddLength(new Uint8Array(await fs.readFile(filePath)));
}

// By default we don't want to restrict that any of the nodes need to be upgraded.
export async function submitRuntimeUpgradeWithRestrictions(
  wasmPath: string,
  semverRestriction?: Record<string, number>,
  percentNodesUpgraded = 0,
  // default to false, because try-runtime feature is not always available.
  tryRuntime = false,
) {
  const runtimeWasm = await readRuntimeWasmFromFile(wasmPath);

  await using chainflip = await getChainflipApi();

  const networkUrl = process.env.CF_NODE_ENDPOINT ?? 'ws://localhost:9944';

  if (tryRuntime) {
    console.log('Running try-runtime before submitting the runtime upgrade.');
    await tryRuntimeUpgrade('last-n', chainflip, networkUrl, wasmPath);
  }

  let versionPercentRestriction;
  if (semverRestriction && percentNodesUpgraded) {
    versionPercentRestriction = [semverRestriction, percentNodesUpgraded];
  } else {
    versionPercentRestriction = undefined;
  }

  console.log('Submitting runtime upgrade.');
  await submitGovernanceExtrinsic((api) =>
    api.tx.governance.chainflipRuntimeUpgrade(versionPercentRestriction, runtimeWasm),
  );

  console.log('Submitted runtime upgrade. Waiting for the runtime upgrade to complete.');

  const event = await Promise.race([
    observeEvent('system:CodeUpdated').event,
    observeEvent('governance:FailedExecution').event,
  ]);

  if (event.name.method === 'FailedExecution') {
    const error = decodeModuleError(event.data[0].Module, chainflip);
    throw Error(`Runtime upgrade failed: ${error}`);
  }

  console.log('Runtime upgrade completed.');
}

export async function submitRuntimeUpgradeWasmPath(wasmPath: string) {
  await submitRuntimeUpgradeWithRestrictions(wasmPath);
}

// Restrictions not provided.
export async function submitRuntimeUpgrade(projectRoot: string, tryRuntime = false) {
  await submitRuntimeUpgradeWithRestrictions(
    `${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
    undefined,
    0,
    tryRuntime,
  );
}
