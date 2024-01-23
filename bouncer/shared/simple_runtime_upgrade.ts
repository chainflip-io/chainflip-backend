import { submitRuntimeUpgrade } from './submit_runtime_upgrade';
import { bumpSpecVersionAgainstNetwork, getNetworkRuntimeVersion } from './utils/spec_version';
import { compileBinaries } from './utils/compile_binaries';

// Do a runtime upgrade using the code in the projectRoot directory.
export async function simpleRuntimeUpgrade(projectRoot: string, tryRuntime = false): Promise<void> {
  const nextSpecVersion = await bumpSpecVersionAgainstNetwork(
    `${projectRoot}/state-chain/runtime/src/lib.rs`,
  );

  await compileBinaries('runtime', projectRoot);

  await submitRuntimeUpgrade(projectRoot, tryRuntime);

  const newSpecVersion = (await getNetworkRuntimeVersion()).specVersion;
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
