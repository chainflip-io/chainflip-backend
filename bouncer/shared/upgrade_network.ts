import { execSync } from 'child_process';
import fs from 'fs/promises';
import * as toml from 'toml';
import path from 'path';
import { SemVerLevel, bumpReleaseVersion } from './bump_release_version';
import { simpleRuntimeUpgrade } from './simple_runtime_upgrade';
import { compareSemVer, getNodesInfo, killEngines, sleep, startEngines } from './utils';
import { bumpSpecVersionAgainstNetwork } from './utils/spec_version';
import { compileBinaries } from './utils/compile_binaries';
import { submitRuntimeUpgradeWithRestrictions } from './submit_runtime_upgrade';
import { execWithLog } from './utils/exec_with_log';
import { submitGovernanceExtrinsic } from './cf_governance';
import { globalLogger as logger } from './utils/logger';

async function readPackageTomlVersion(projectRoot: string): Promise<string> {
  const data = await fs.readFile(path.join(projectRoot, '/state-chain/runtime/Cargo.toml'), 'utf8');
  const parsedData = toml.parse(data);
  const version = parsedData.package.version;
  return version;
}

// The javascript version of state-chain/primitives/src/lib.rs - SemVer::is_compatible_with()
function isCompatibleWith(semVer1: string, semVer2: string) {
  const [major1, minor1] = semVer1.split('.').map(Number);
  const [major2, minor2] = semVer2.split('.').map(Number);

  return major1 === major2 && minor1 === minor2;
}

// Create a git workspace in the tmp/ directory and check out the specified commit.
// Remember to delete it when you're done!
function createGitWorkspaceAt(nextVersionWorkspacePath: string, toGitRef: string) {
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

function killOldNodes() {
  logger.info('Killing the old node.');

  try {
    const execOutput = execSync(
      `kill $(ps -o pid -o comm | grep chainflip-node | awk '{print $1}')`,
    );
    logger.info('Kill node exec output:', execOutput.toString());
  } catch (e) {
    logger.info('Error killing node: ' + e);
    throw e;
  }

  logger.info('Killed old node');
}

async function startBrokerAndLpApi(localnetInitPath: string, binaryPath: string, keysDir: string) {
  logger.info('Starting new broker and lp-api.');

  await execWithLog(
    `${localnetInitPath}/scripts/start-broker-api.sh ${binaryPath}`,
    'start-broker-api',
    {
      KEYS_DIR: keysDir,
    },
  );

  await execWithLog(`${localnetInitPath}/scripts/start-lp-api.sh ${binaryPath}`, 'start-lp-api', {
    KEYS_DIR: keysDir,
  });

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

async function startDepositMonitor(localnetInitPath: string) {
  logger.info('Starting up deposit-monitor.');

  await execWithLog(
    `${localnetInitPath}/scripts/start-deposit-monitor.sh`,
    'start-deposit-monitor',
    {
      LOCALNET_INIT_DIR: `${localnetInitPath}`,
      DEPOSIT_MONITOR_CONTAINER: 'deposit-monitor',
      DOCKER_COMPOSE_CMD: 'docker compose',
      additional_docker_compose_up_args: '--quiet-pull',
    },
  );
}

async function compatibleUpgrade(
  localnetInitPath: string,
  binaryPath: string,
  runtimePath: string,
  numberOfNodes: 1 | 3,
) {
  await submitRuntimeUpgradeWithRestrictions(logger, runtimePath, undefined, undefined, true);

  killOldNodes();

  const KEYS_DIR = `${localnetInitPath}/keys`;

  const { SELECTED_NODES, nodeCount } = await getNodesInfo(numberOfNodes);

  await execWithLog(`${localnetInitPath}/scripts/start-all-nodes.sh`, 'start-all-nodes', {
    INIT_RPC_PORT: `9944`,
    KEYS_DIR,
    NODE_COUNT: nodeCount,
    SELECTED_NODES,
    LOCALNET_INIT_DIR: localnetInitPath,
    BINARY_ROOT_PATH: binaryPath,
  });

  // wait for nodes to be ready
  await sleep(20000);

  // engines crashed when node shutdown, so restart them.
  await execWithLog(
    `${localnetInitPath}/scripts/start-all-engines.sh`,
    'start-all-engines-post-upgrade',
    {
      INIT_RUN: 'false',
      LOG_SUFFIX: '-post-upgrade',
      NODE_COUNT: nodeCount,
      SELECTED_NODES,
      LOCALNET_INIT_DIR: localnetInitPath,
      BINARY_ROOT_PATH: binaryPath,
    },
  );

  await startBrokerAndLpApi(localnetInitPath, binaryPath, KEYS_DIR);

  await startDepositMonitor(localnetInitPath);
}

async function incompatibleUpgradeNoBuild(
  localnetInitPath: string,
  binaryPath: string,
  runtimePath: string,
  numberOfNodes: 1 | 3,
) {
  // We need to kill the engine process before starting the new engine (engine-runner)
  // Since the new engine contains the old one.
  logger.info('Killing the old engines');
  await killEngines();
  await startEngines(localnetInitPath, binaryPath, numberOfNodes);

  await submitRuntimeUpgradeWithRestrictions(logger, runtimePath, undefined, undefined, true);

  logger.info(
    'Check that the old engine has now shut down, and that the new engine is now running.',
  );

  // TODO: add some tests here. After this point. If the upgrade doesn't work.
  // but below, we effectively restart the engine before running any tests it's possible that
  // we don't catch the error here.

  // Ensure the runtime upgrade is finalised.
  await sleep(10000);

  // We're going to take down the node, so we don't want them to be suspended.
  await submitGovernanceExtrinsic((api) =>
    api.tx.reputation.setPenalty('MissedAuthorshipSlot', {
      reputation: 100,
      suspension: 0,
    }),
  );

  logger.info('Submitted extrinsic to set suspension for MissedAuthorship slot to 0');
  // Ensure extrinsic gets in.
  await sleep(12000);

  killOldNodes();

  // let them shutdown
  await sleep(4000);

  logger.info('Old broker and LP-API have crashed since we killed the node.');

  logger.info('Starting the new node');

  const KEYS_DIR = `${localnetInitPath}/keys`;

  const { SELECTED_NODES, nodeCount } = await getNodesInfo(numberOfNodes);
  await execWithLog(`${localnetInitPath}/scripts/start-all-nodes.sh`, 'start-all-nodes', {
    INIT_RPC_PORT: `9944`,
    KEYS_DIR,
    NODE_COUNT: nodeCount,
    SELECTED_NODES,
    LOCALNET_INIT_DIR: localnetInitPath,
    BINARY_ROOT_PATH: binaryPath,
  });

  await sleep(20000);

  // Set missed authorship suspension back to 100/150 after nodes back up.
  await submitGovernanceExtrinsic((api) =>
    api.tx.reputation.setPenalty('MissedAuthorshipSlot', {
      reputation: 100,
      suspension: 150,
    }),
  );

  const output = execSync("ps -o pid -o comm | grep chainflip-node | awk '{print $1}'");
  logger.info('New node PID: ' + output.toString());

  // Restart the engines
  await execWithLog(
    `${localnetInitPath}/scripts/start-all-engines.sh`,
    'start-all-engines-post-upgrade',
    {
      INIT_RUN: 'false',
      LOG_SUFFIX: '-post-upgrade',
      NODE_COUNT: nodeCount,
      SELECTED_NODES,
      LOCALNET_INIT_DIR: localnetInitPath,
      BINARY_ROOT_PATH: binaryPath,
    },
  );

  await sleep(4000);

  await startBrokerAndLpApi(localnetInitPath, binaryPath, KEYS_DIR);
  logger.info('Started new broker and lp-api.');

  await startDepositMonitor(localnetInitPath);
  logger.info('Started new deposit monitor.');
}

async function incompatibleUpgrade(
  // could we pass localnet/init instead of this.
  localnetInitPath: string,
  nextVersionWorkspacePath: string,
  numberOfNodes: 1 | 3,
) {
  await bumpSpecVersionAgainstNetwork(
    logger,
    `${nextVersionWorkspacePath}/state-chain/runtime/src/lib.rs`,
  );

  await compileBinaries('all', nextVersionWorkspacePath);

  await incompatibleUpgradeNoBuild(
    localnetInitPath,
    `${nextVersionWorkspacePath}/target/release`,
    `${nextVersionWorkspacePath}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
    numberOfNodes,
  );
}

// Upgrades a bouncer network from the commit currently running on localnet to the provided git reference (commit, branch, tag).
// If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.
// Only the incompatible upgrade requires the number of nodes.
export async function upgradeNetworkGit(
  toGitRef: string,
  bumpByIfEqual: SemVerLevel = 'patch',
  numberOfNodes: 1 | 3 = 1,
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

  const isCompatible = isCompatibleWith(fromTomlVersion, newToTomlVersion);
  logger.info('Is compatible: ' + isCompatible);

  const localnetInitPath = `${currentVersionWorkspacePath}/localnet/init`;
  if (isCompatible) {
    logger.info('The versions are compatible.');
    await simpleRuntimeUpgrade(logger, nextVersionWorkspacePath, true);

    // TODO: Add restart nodes support, as in the prebuilt case.

    logger.info('Upgrade complete.');
  } else if (!isCompatible) {
    logger.info('The versions are incompatible.');
    await incompatibleUpgrade(localnetInitPath, nextVersionWorkspacePath, numberOfNodes);
  }

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
  } else if (isCompatibleWith(cleanOldVersion, nodeVersion)) {
    logger.info('The versions are compatible.');
    await compatibleUpgrade(localnetInitPath, binariesPath, runtimePath, numberOfNodes);
  } else {
    logger.info('The versions are incompatible.');
    await incompatibleUpgradeNoBuild(localnetInitPath, binariesPath, runtimePath, numberOfNodes);
  }

  logger.info('Upgrade complete.');
}
