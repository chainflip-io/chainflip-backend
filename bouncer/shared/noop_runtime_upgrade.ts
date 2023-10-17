// Do a runtime upgrade that does nothing - the runtime should be identical except for the `spec_version` field.
// Needs to be run from the bouncer directory.
import { execSync } from 'node:child_process';

import { submitRuntimeUpgrade } from './submit_runtime_upgrade';
import { jsonRpc } from './json_rpc';
import { getChainflipApi, observeEvent } from './utils';
import { promptUser } from './prompt_user';

async function getCurrentSpecVersion(): Promise<number> {
  return Number((await jsonRpc('state_getRuntimeVersion', [], 9944)).specVersion);
}

export async function noopRuntimeUpgrade(): Promise<void> {
  const chainflip = await getChainflipApi();

  const currentSpecVersion = await getCurrentSpecVersion();

  console.log('Current spec_version: ' + currentSpecVersion);

  const nextSpecVersion = currentSpecVersion + 1;

  await promptUser(
    'Go to state-chain/runtime/src/lib.rs and then in that file go to #[sp_version::runtime_version]. Set the `spec_version` to ' +
      nextSpecVersion +
      ' and save the file.',
  );

  console.log('Building the new runtime');
  execSync('cd ../state-chain/runtime && cargo build --release');

  console.log('Built the new runtime. Applying runtime upgrade.');
  await submitRuntimeUpgrade(
    '../target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm',
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
