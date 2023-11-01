import { submitRuntimeUpgrade } from './submit_runtime_upgrade';
import { bumpSpecVersionAgainstNetwork, getCurrentSpecVersion } from './utils/bump_spec_version';
import { compileBinaries } from './utils/compile_binaries';

// Do a runtime upgrade using the code in the projectRoot directory.
export async function simpleRuntimeUpgrade(projectRoot: string): Promise<void> {

  const nextSpecVersion = await bumpSpecVersionAgainstNetwork(projectRoot);

  await compileBinaries('runtime', projectRoot);

  await submitRuntimeUpgrade(
    `${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
  );

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
