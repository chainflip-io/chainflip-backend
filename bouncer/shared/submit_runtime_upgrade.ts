import { compactAddLength } from '@polkadot/util';
import { promises as fs } from 'fs';
import { submitGovernanceExtrinsic } from './cf_governance';
import { getChainflipApi, observeEvent } from '../shared/utils';

async function readRuntimeWasmFromFile(filePath: string): Promise<Uint8Array> {
  return compactAddLength(new Uint8Array(await fs.readFile(filePath)));
}

// By default we don't want to restrict that any of the nodes need to be upgraded.
export async function submitRuntimeUpgrade(
  wasmPath: string,
  semverRestriction?: Record<string, number>,
  percentNodesUpgraded = 0,
) {
  const runtimeWasm = await readRuntimeWasmFromFile(wasmPath);

  console.log('Submitting runtime upgrade.');
  const chainflip = await getChainflipApi();

  let versionPercentRestriction;
  if (semverRestriction && percentNodesUpgraded) {
    versionPercentRestriction = [semverRestriction, percentNodesUpgraded];
  } else {
    versionPercentRestriction = undefined;
  }

  await submitGovernanceExtrinsic(
    chainflip.tx.governance.chainflipRuntimeUpgrade(versionPercentRestriction, runtimeWasm),
  );

  await observeEvent('system:CodeUpdated', chainflip);

  console.log('Runtime upgrade completed.');
}
