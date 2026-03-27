import { execSync } from 'child_process';
import fs from 'fs/promises';
import * as toml from 'toml';
import path from 'path';
import { SemVerLevel, bumpReleaseVersion } from 'shared/bump_release_version';
import {
  compareSemVer,
  killEngines,
  killNodes,
  sleep,
  startEngines,
  startNodes,
  waitForProcessExit,
} from 'shared/utils';
import { bumpSpecVersionAgainstNetwork } from 'shared/utils/spec_version';
import { compileBinaries } from 'shared/utils/compile_binaries';
import { submitRuntimeUpgradeWithRestrictions } from 'shared/submit_runtime_upgrade';
import { execWithLog } from 'shared/utils/exec_with_log';
import { clearChainflipApiCache, clearSubscribeHeadsCache } from 'shared/utils/substrate';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { Logger } from 'pino';
import { globalLogger } from './utils/logger';

async function readPackageTomlVersion(projectRoot: string): Promise<string> {
  const data = await fs.readFile(path.join(projectRoot, '/state-chain/runtime/Cargo.toml'), 'utf8');
  const parsedData = toml.parse(data);
  const version = parsedData.package.version;
  return version;
}

// Create a git workspace in the tmp/ directory and check out the specified commit.
// Remember to delete it when you're done!
function createGitWorkspaceAt(
  nextVersionWorkspacePath: string,
  toGitRef: string,
  logger: Logger = globalLogger,
) {
  try {
    // Create a directory for the new workspace
    execSync(`mkdir -p ${nextVersionWorkspacePath}`);

    // Create a new workspace using git worktree.
    execSync(`git worktree add ${nextVersionWorkspacePath}`);

    // Navigate to the new workspace and checkout the specific commit
    execSync(`cd ${nextVersionWorkspacePath} && git checkout ${toGitRef}`);

    logger.info('Commit checked out successfully in new workspace.');
  } catch (error) {
    console.error(`Error: ${error}`);
  }
}

async function startBrokerAndLpApi(
  localnetInitPath: string,
  binaryPath: string,
  keysDir: string,
  logger: Logger = globalLogger,
) {
  logger.info('Starting new broker and lp-api.');

  await execWithLog(
    `${localnetInitPath}/scripts/start-broker-api.sh`,
    [`${binaryPath}`],
    'start-broker-api',
    {
      KEYS_DIR: keysDir,
    },
  );

  await execWithLog(
    `${localnetInitPath}/scripts/start-lp-api.sh`,
    [`${binaryPath}`],
    'start-lp-api',
    {
      KEYS_DIR: keysDir,
    },
  );

  await sleep(10000);

  for (const [process, port] of [
    ['broker-api', 10997],
    ['lp-api', 10589],
  ]) {
    try {
      const pid = execSync(`lsof -t -i:${port}`);
      logger.info(`New ${process} PID: ${pid.toString()}`);
    } catch (e) {
      console.error(`Error starting ${process}: ${e}`);
      throw e;
    }
  }
}

async function startDepositMonitor(localnetInitPath: string, logger: Logger = globalLogger) {
  logger.info('Starting up deposit-monitor.');

  await execWithLog(
    `${localnetInitPath}/scripts/start-deposit-monitor.sh`,
    [],
    'start-deposit-monitor',
    {
      LOCALNET_INIT_DIR: `${localnetInitPath}`,
      DEPOSIT_MONITOR_CONTAINER: 'deposit-monitor',
      DOCKER_COMPOSE_CMD: 'docker compose',
      additional_docker_compose_up_args: '--quiet-pull',
    },
  );
}

/// Upgrades a running localnet from old node and engine binaries to new ones,
/// without performing a runtime upgrade. The chain state and on-chain runtime
/// version are preserved — only the node and engine processes are replaced.
///
/// This simulates a binary-only upgrade (validators update their software
/// before a governance runtime upgrade is submitted).
export async function upgradeBinaries(
  localnetInitPath: string,
  newBinaryPath: string,
  numberOfNodes: 1 | 3 = 1,
  logger: Logger = globalLogger,
) {
  // Kill engines first so they don't try to submit extrinsics while the node
  // is shutting down.
  await killEngines(logger);

  await killNodes(logger);

  // start the new nodes, this also waits until they are healthy
  await startNodes(localnetInitPath, newBinaryPath, numberOfNodes, '-binaries-upgrade');

  const newNodePids = execSync("ps -o pid -o comm | grep chainflip-node | awk '{print $1}'");
  logger.info('New node PID(s): ' + newNodePids.toString().trim());

  // The API connections opened against the old node are stale after a restart.
  clearChainflipApiCache();
  clearSubscribeHeadsCache();

  // Start the new engines
  await startEngines(localnetInitPath, newBinaryPath, numberOfNodes, '-binaries-upgrade');

  logger.info(
    'Binary upgrade complete. binaries updated but on-chain runtime version is unchanged.',
  );
}

export async function restartDmLpBrokerApis(
  localnetInitPath: string,
  binaryPath: string,
  logger: Logger = globalLogger,
) {
  const KEYS_DIR = `${localnetInitPath}/keys`;

  // Kill broker and lp-api processes and wait for them to exit.
  for (const processName of ['chainflip-broker-api', 'chainflip-lp-api']) {
    try {
      logger.info(`killing ${processName}`);
      execSync(`pkill -x ${processName}`);
      await waitForProcessExit(processName);
    } catch {
      logger.info(`${processName} was not running, skipping`);
    }
  }
  await startBrokerAndLpApi(localnetInitPath, binaryPath, KEYS_DIR, logger);
  logger.info('Started new broker and lp-api.');

  // No need to docker compose stop the deposit monitor because docker compose up would do
  // it if the DM image or env have changed
  await startDepositMonitor(localnetInitPath, logger);
  logger.info('Started new deposit monitor.');
}

async function upgradeNoBuild(
  localnetInitPath: string,
  binaryPath: string,
  runtimePath: string,
  numberOfNodes: 1 | 3 = 1,
  logger: Logger = globalLogger,
) {
  const cf = await newChainflipIO(logger, []);

  await upgradeBinaries(localnetInitPath, binaryPath, numberOfNodes, logger);

  await submitRuntimeUpgradeWithRestrictions(cf, runtimePath, undefined, undefined, true);

  await restartDmLpBrokerApis(localnetInitPath, binaryPath, logger);
}

// Upgrades a bouncer network from the commit currently running on localnet to the provided git reference (commit, branch, tag).
// If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.
export async function upgradeNetworkGit(
  toGitRef: string,
  bumpByIfEqual: SemVerLevel = 'patch',
  numberOfNodes: 1 | 3 = 1,
  logger: Logger = globalLogger,
) {
  logger.info('Upgrading network to git ref: ' + toGitRef);

  const currentVersionWorkspacePath = path.dirname(process.cwd());

  const fromTomlVersion = await readPackageTomlVersion(currentVersionWorkspacePath);
  logger.info("Version we're upgrading from: " + fromTomlVersion);

  // tmp/ is ignored in the bouncer .gitignore file.
  const nextVersionWorkspacePath = path.join(process.cwd(), 'tmp/upgrade-network');

  logger.info('Creating a new git workspace at: ' + nextVersionWorkspacePath);
  createGitWorkspaceAt(nextVersionWorkspacePath, toGitRef);

  const toTomlVersion = await readPackageTomlVersion(`${nextVersionWorkspacePath}`);
  logger.info("Version of commit we're upgrading to: " + toTomlVersion);

  if (compareSemVer(fromTomlVersion, toTomlVersion) === 'greater') {
    throw new Error(
      "The version we're upgrading to is older than the version we're upgrading from. Ensure you selected the correct commits.",
    );
  }

  // Now we need to bump the TOML versions if required, to ensure the `CurrentReleaseVersion` in the environment pallet is correct.
  if (fromTomlVersion === toTomlVersion) {
    logger.info('Versions are equal, bumping by: ' + bumpByIfEqual);
    await bumpReleaseVersion(bumpByIfEqual, nextVersionWorkspacePath);
  } else {
    logger.info('Versions are not equal, no need to bump.');
  }

  const newToTomlVersion = await readPackageTomlVersion(path.join(nextVersionWorkspacePath));
  logger.info("Version we're upgrading to: " + newToTomlVersion);

  const localnetInitPath = `${currentVersionWorkspacePath}/localnet/init`;

  // Bump version and compile binaries
  await bumpSpecVersionAgainstNetwork(
    logger,
    `${nextVersionWorkspacePath}/state-chain/runtime/src/lib.rs`,
  );
  await compileBinaries('all', nextVersionWorkspacePath);

  await upgradeNoBuild(
    localnetInitPath,
    `${nextVersionWorkspacePath}/target/release`,
    `${nextVersionWorkspacePath}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
    numberOfNodes,
  );

  logger.info('Cleaning up...');
  execSync(`cd ${nextVersionWorkspacePath} && git worktree remove . --force`);
  logger.info('Done.');
}

export async function upgradeNetworkPrebuilt(
  // Directory where the node and CFE binaries of the new version are located
  binariesPath: string,
  // Path to the runtime we will upgrade to
  runtimePath: string,
  localnetInitPath: string,
  oldVersion: string,
  numberOfNodes: 1 | 3 = 1,
  logger: Logger = globalLogger,
) {
  const versionRegex = /\d+\.\d+\.\d+/;

  logger.info("Raw version we're upgrading from: " + oldVersion);

  let cleanOldVersion = oldVersion;
  if (versionRegex.test(cleanOldVersion)) {
    cleanOldVersion = oldVersion.match(versionRegex)?.[0] ?? '';
  }

  logger.info("Version we're upgrading from: " + cleanOldVersion);

  const nodeBinaryVersion = execSync(`${binariesPath}/chainflip-node --version`).toString();
  const nodeVersion = nodeBinaryVersion.match(versionRegex)?.[0] ?? '';
  logger.info("Node version we're upgrading to: " + nodeVersion);

  // We use nodeVersion as a proxy for the cfe version since they are updated together.
  // And getting the cfe version involves ensuring the dylib is available.
  if (compareSemVer(cleanOldVersion, nodeVersion) === 'greater') {
    throw new Error(
      "The version we're upgrading to is older than the version we're upgrading from. Ensure you selected the correct binaries.",
    );
  }

  if (cleanOldVersion === nodeVersion) {
    throw Error(
      'The versions are the same. No need to upgrade. Please provide a different version.',
    );
  } else {
    await upgradeNoBuild(localnetInitPath, binariesPath, runtimePath, numberOfNodes);
  }

  logger.info('Upgrade complete.');
}
