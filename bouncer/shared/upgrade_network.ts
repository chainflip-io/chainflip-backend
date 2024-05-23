import { execSync } from 'child_process';
import fs from 'fs/promises';
import * as toml from 'toml';
import path from 'path';
import { SemVerLevel, bumpReleaseVersion } from './bump_release_version';
import { simpleRuntimeUpgrade } from './simple_runtime_upgrade';
import { compareSemVer, sleep } from './utils';
import { bumpSpecVersionAgainstNetwork } from './utils/spec_version';
import { compileBinaries } from './utils/compile_binaries';
import { submitRuntimeUpgradeWithRestrictions } from './submit_runtime_upgrade';
import { execWithLog } from './utils/exec_with_log';
import { submitGovernanceExtrinsic } from './cf_governance';

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

    console.log('Commit checked out successfully in new workspace.');
  } catch (error) {
    console.error(`Error: ${error}`);
  }
}

function killOldNodes() {
  console.log('Killing the old node.');

  try {
    const execOutput = execSync(
      `kill $(ps -o pid -o comm | grep chainflip-node | awk '{print $1}')`,
    );
    console.log('Kill node exec output:', execOutput.toString());
  } catch (e) {
    console.log('Error killing node: ' + e);
    throw e;
  }

  console.log('Killed old node');
}

async function startBrokerAndLpApi(localnetInitPath: string, binaryPath: string, keysDir: string) {
  console.log('Starting new broker and lp-api.');

  execWithLog(`${localnetInitPath}/scripts/start-broker-api.sh ${binaryPath}`, 'start-broker-api', {
    keysDir,
  });

  execWithLog(`${localnetInitPath}/scripts/start-lp-api.sh ${binaryPath}`, 'start-lp-api', {
    keysDir,
  });

  await sleep(10000);

  for (const [process, port] of [
    ['broker-api', 10997],
    ['lp-api', 10589],
  ]) {
    try {
      const pid = execSync(`lsof -t -i:${port}`);
      console.log(`New ${process} PID: ${pid.toString()}`);
    } catch (e) {
      console.error(`Error starting ${process}: ${e}`);
      throw e;
    }
  }
}

async function compatibleUpgrade(
  localnetInitPath: string,
  binaryPath: string,
  runtimePath: string,
  numberOfNodes: 1 | 3,
) {
  await submitRuntimeUpgradeWithRestrictions(runtimePath, undefined, undefined, true);

  killOldNodes();

  const KEYS_DIR = `${localnetInitPath}/keys`;

  const nodeCount = numberOfNodes + '-node';

  const SELECTED_NODES = numberOfNodes === 1 ? 'bashful' : 'bashful doc dopey';

  execWithLog(`${localnetInitPath}/scripts/start-all-nodes.sh`, 'start-all-nodes', {
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
  execWithLog(
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
}

async function incompatibleUpgradeNoBuild(
  localnetInitPath: string,
  binaryPath: string,
  runtimePath: string,
  numberOfNodes: 1 | 3,
) {
  const SELECTED_NODES = numberOfNodes === 1 ? 'bashful' : 'bashful doc dopey';

  // We need to kill the engine process before starting the new engine (engine-runner)
  // Since the new engine contains the old one.
  console.log('Killing the old engines');
  execSync(`kill $(ps aux | grep engine-runner | grep -v grep | awk '{print $2}')`);

  console.log('Starting all the engines');

  const nodeCount = numberOfNodes + '-node';
  execWithLog(`${localnetInitPath}/scripts/start-all-engines.sh`, 'start-all-engines-pre-upgrade', {
    INIT_RUN: 'false',
    LOG_SUFFIX: '-pre-upgrade',
    NODE_COUNT: nodeCount,
    SELECTED_NODES,
    LOCALNET_INIT_DIR: localnetInitPath,
    BINARY_ROOT_PATH: binaryPath,
  });

  await sleep(7000);

  console.log('Engines started');

  await submitRuntimeUpgradeWithRestrictions(runtimePath, undefined, undefined, false);

  console.log(
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

  console.log('Submitted extrinsic to set suspension for MissedAuthorship slot to 0');
  // Ensure extrinsic gets in.
  await sleep(12000);

  killOldNodes();

  // let them shutdown
  await sleep(4000);

  console.log('Old broker and LP-API have crashed since we killed the node.');

  console.log('Starting the new node');

  const KEYS_DIR = `${localnetInitPath}/keys`;

  execWithLog(`${localnetInitPath}/scripts/start-all-nodes.sh`, 'start-all-nodes', {
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
  console.log('New node PID: ' + output.toString());

  // Restart the engines
  execWithLog(
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

  console.log('Started new broker and lp-api.');
}

async function incompatibleUpgrade(
  // could we pass localnet/init instead of this.
  localnetInitPath: string,
  nextVersionWorkspacePath: string,
  numberOfNodes: 1 | 3,
) {
  await bumpSpecVersionAgainstNetwork(
    `${nextVersionWorkspacePath}/state-chain/runtime/src/lib.rs`,
    9944,
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
  console.log('Upgrading network to git ref: ' + toGitRef);

  const currentVersionWorkspacePath = path.dirname(process.cwd());

  const fromTomlVersion = await readPackageTomlVersion(currentVersionWorkspacePath);
  console.log("Version we're upgrading from: " + fromTomlVersion);

  // tmp/ is ignored in the bouncer .gitignore file.
  const nextVersionWorkspacePath = path.join(process.cwd(), 'tmp/upgrade-network');

  console.log('Creating a new git workspace at: ' + nextVersionWorkspacePath);
  createGitWorkspaceAt(nextVersionWorkspacePath, toGitRef);

  const toTomlVersion = await readPackageTomlVersion(`${nextVersionWorkspacePath}`);
  console.log("Version of commit we're upgrading to: " + toTomlVersion);

  if (compareSemVer(fromTomlVersion, toTomlVersion) === 'greater') {
    throw new Error(
      "The version we're upgrading to is older than the version we're upgrading from. Ensure you selected the correct commits.",
    );
  }

  // Now we need to bump the TOML versions if required, to ensure the `CurrentReleaseVersion` in the environment pallet is correct.
  if (fromTomlVersion === toTomlVersion) {
    console.log('Versions are equal, bumping by: ' + bumpByIfEqual);
    await bumpReleaseVersion(bumpByIfEqual, nextVersionWorkspacePath);
  } else {
    console.log('Versions are not equal, no need to bump.');
  }

  const newToTomlVersion = await readPackageTomlVersion(path.join(nextVersionWorkspacePath));
  console.log("Version we're upgrading to: " + newToTomlVersion);

  const isCompatible = isCompatibleWith(fromTomlVersion, newToTomlVersion);
  console.log('Is compatible: ' + isCompatible);

  const localnetInitPath = `${currentVersionWorkspacePath}/localnet/init`;
  if (isCompatible) {
    console.log('The versions are compatible.');
    await simpleRuntimeUpgrade(nextVersionWorkspacePath, true);

    // TODO: Add restart nodes support, as in the prebuilt case.

    console.log('Upgrade complete.');
  } else if (!isCompatible) {
    console.log('The versions are incompatible.');
    await incompatibleUpgrade(
      localnetInitPath,
      nextVersionWorkspacePath,
      numberOfNodes,
    );
  }

  console.log('Cleaning up...');
  execSync(`cd ${nextVersionWorkspacePath} && git worktree remove . --force`);
  console.log('Done.');
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

  console.log("Raw version we're upgrading from: " + oldVersion);

  let cleanOldVersion = oldVersion;
  if (versionRegex.test(cleanOldVersion)) {
    cleanOldVersion = oldVersion.match(versionRegex)[0];
  }

  console.log("Version we're upgrading from: " + cleanOldVersion);

  const nodeBinaryVersion = execSync(`${binariesPath}/chainflip-node --version`).toString();
  const nodeVersion = nodeBinaryVersion.match(versionRegex)[0];
  console.log("Node version we're upgrading to: " + nodeVersion);

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
    console.log('The versions are compatible.');
    await compatibleUpgrade(
      localnetInitPath,
      binariesPath,
      runtimePath,
      numberOfNodes,
    );
  } else {
    console.log('The versions are incompatible.');
    await incompatibleUpgradeNoBuild(
      localnetInitPath,
      binariesPath,
      runtimePath,
      numberOfNodes,
    );
  }

  console.log('Upgrade complete.');
}
