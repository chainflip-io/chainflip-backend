import { submitRuntimeUpgrade } from './submit_runtime_upgrade';
import { jsonRpc } from './json_rpc';
import { getChainflipApi, observeEvent } from './utils';
import { bumpSpecVersion } from './utils/bump_spec_version';
import { compileBinaries } from './utils/compile_binaries';

async function getCurrentSpecVersion(): Promise<number> {
  return Number((await jsonRpc('state_getRuntimeVersion', [], 9944)).specVersion);
}

// Do a runtime upgrade using the code in the projectRoot directory.
export async function simpleRuntimeUpgrade(projectRoot: string): Promise<void> {
  const chainflip = await getChainflipApi();
  const currentSpecVersion = await getCurrentSpecVersion();
  console.log('Current spec_version: ' + currentSpecVersion);
  const nextSpecVersion = currentSpecVersion + 1;
  bumpSpecVersion(`${projectRoot}/state-chain/runtime/src/lib.rs`, nextSpecVersion);

  await compileBinaries('runtime', projectRoot);

  console.log('Applying runtime upgrade.');
  await submitRuntimeUpgrade(
    `${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
  );

  await observeEvent('system:CodeUpdated', chainflip);

  const newSpecVersion = await getCurrentSpecVersion();
  console.log('New spec_version: ' + newSpecVersion);

  if (newSpecVersion !== nextSpecVersion) {
    console.error(
      'After submitting the runtime upgrade, the new spec_version is not what we expected. Expected: ' +
        nextSpecVersion +
        ' Got: ' +
        newSpecVersion,
    );
  }

  console.log('Runtime upgrade complete.');
}
