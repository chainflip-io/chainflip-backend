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

async function incompatibleUpgradeNoBuild(
  localnetInitPath: string,
  binaryPath: string,
  runtimePath: string,
  numberOfNodes: 1 | 3,
) {
  let selectedNodes;
  if (numberOfNodes === 1) {
    selectedNodes = ['bashful'];
  } else if (numberOfNodes === 3) {
    selectedNodes = ['bashful', 'doc', 'dopey'];
  } else {
    throw new Error('Invalid number of nodes');
  }

  console.log('Starting all the engines');

  const nodeCount = numberOfNodes + '-node';
  execSync(
    `LOG_SUFFIX="-upgrade" NODE_COUNT=${nodeCount} SELECTED_NODES="${selectedNodes.join(
      ' ',
    )}" LOCALNET_INIT_DIR=${localnetInitPath} BINARY_ROOT_PATH=${binaryPath} ${localnetInitPath}/scripts/start-all-engines.sh`,
  );

  await sleep(7000);

  console.log('Engines started');

  await submitRuntimeUpgradeWithRestrictions(runtimePath, undefined, undefined, true);

  console.log(
    'Check that the old engine has now shut down, and that the new engine is now running.',
  );

  execSync(`kill $(lsof -t -i:10997)`);
  execSync(`kill $(lsof -t -i:10589)`);
  console.log('Stopped old broker and lp-api. Starting the new ones.');

  // Wait for the old broker and lp-api to shut down, and ensure the runtime upgrade is finalised.
  await sleep(22000);

  const KEYS_DIR = `${localnetInitPath}/keys`;
  execSync(`KEYS_DIR=${KEYS_DIR} ${localnetInitPath}/scripts/start-broker-api.sh ${binaryPath}`);
  execSync(`KEYS_DIR=${KEYS_DIR} ${localnetInitPath}/scripts/start-lp-api.sh ${binaryPath}`);
  await sleep(6000);
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

  if (isCompatible) {
    console.log('The versions are compatible.');
    await simpleRuntimeUpgrade(nextVersionWorkspacePath, true);
    console.log('Upgrade complete.');
  } else if (!isCompatible) {
    console.log('The versions are incompatible.');
    await incompatibleUpgrade(
      `${currentVersionWorkspacePath}/localnet/init`,
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

  console.log("Version we're upgrading from: " + oldVersion);

  let cleanOldVersion = oldVersion;
  if (!versionRegex.test(cleanOldVersion)) {
    cleanOldVersion = oldVersion.match(versionRegex)[0];
  }

  const cfeBinaryVersion = execSync(`${binariesPath}/chainflip-engine --version`).toString();
  const cfeVersion = cfeBinaryVersion.match(versionRegex)[0];
  console.log("CFE version we're upgrading to: " + cfeVersion);

  const nodeBinaryVersion = execSync(`${binariesPath}/chainflip-node --version`).toString();
  const nodeVersion = nodeBinaryVersion.match(versionRegex)[0];
  console.log("Node version we're upgrading to: " + nodeVersion);

  if (cfeVersion !== nodeVersion) {
    throw new Error(
      "The CFE version and the node version don't match. Ensure you selected the correct binaries.",
    );
  }

  if (compareSemVer(cleanOldVersion, cfeVersion) === 'greater') {
    throw new Error(
      "The version we're upgrading to is older than the version we're upgrading from. Ensure you selected the correct binaries.",
    );
  }

  const isCompatible = isCompatibleWith(cleanOldVersion, cfeVersion);

  if (!isCompatible) {
    console.log('The versions are incompatible.');
    await incompatibleUpgradeNoBuild(localnetInitPath, binariesPath, runtimePath, numberOfNodes);
  } else {
    console.log('The versions are compatible.');
    await submitRuntimeUpgradeWithRestrictions(runtimePath, undefined, undefined, true);
  }

  console.log('Upgrade complete.');
}
