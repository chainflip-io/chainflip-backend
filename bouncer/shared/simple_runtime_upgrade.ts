import { submitRuntimeUpgrade } from 'shared/submit_runtime_upgrade';
import { bumpSpecVersionAgainstNetwork, getNetworkRuntimeVersion } from 'shared/utils/spec_version';
import { compileBinaries } from 'shared/utils/compile_binaries';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { Logger } from 'shared/utils/logger';

// Do a runtime upgrade using the code in the projectRoot directory.
export async function simpleRuntimeUpgrade(
  logger: Logger,
  projectRoot: string,
  tryRuntime = false,
): Promise<void> {
  const cf = await newChainflipIO(logger, []);

  const nextSpecVersion = await bumpSpecVersionAgainstNetwork(
    cf.logger,
    `${projectRoot}/state-chain/runtime/src/lib.rs`,
  );

  await compileBinaries('runtime', projectRoot);

  await submitRuntimeUpgrade(cf, projectRoot, tryRuntime);

  const newSpecVersion = (await getNetworkRuntimeVersion(cf.logger)).specVersion;
  cf.debug('New spec_version: ' + newSpecVersion);

  if (newSpecVersion !== nextSpecVersion) {
    cf.error(
      `After submitting the runtime upgrade, the new spec_version is not what we expected. Expected: ${nextSpecVersion}, Got: ${newSpecVersion}`,
    );
  }

  cf.info('Runtime upgrade complete.');
}
