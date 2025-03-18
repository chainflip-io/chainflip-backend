import { submitRuntimeUpgrade } from './submit_runtime_upgrade';
import { bumpSpecVersionAgainstNetwork, getNetworkRuntimeVersion } from './utils/spec_version';
import { compileBinaries } from './utils/compile_binaries';
import { Logger } from './utils/logger';

// Do a runtime upgrade using the code in the projectRoot directory.
export async function simpleRuntimeUpgrade(
  logger: Logger,
  projectRoot: string,
  tryRuntime = false,
): Promise<void> {
  const nextSpecVersion = await bumpSpecVersionAgainstNetwork(
    logger,
    `${projectRoot}/state-chain/runtime/src/lib.rs`,
  );

  await compileBinaries('runtime', projectRoot);

  await submitRuntimeUpgrade(logger, projectRoot, tryRuntime);

  const newSpecVersion = (await getNetworkRuntimeVersion(logger)).specVersion;
  logger.debug('New spec_version: ' + newSpecVersion);

  if (newSpecVersion !== nextSpecVersion) {
    logger.error(
      `After submitting the runtime upgrade, the new spec_version is not what we expected. Expected: ${nextSpecVersion}, Got: ${newSpecVersion}`,
    );
  }

  logger.info('Runtime upgrade complete.');
}
