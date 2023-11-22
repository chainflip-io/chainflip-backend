import { execSync } from 'child_process';
import fs from 'fs/promises';
import * as toml from 'toml';
import path from 'path';
import { SemVerLevel, bumpReleaseVersion } from './bump_release_version';
import { simpleRuntimeUpgrade } from './simple_runtime_upgrade';
import { compareSemVer, sleep } from './utils';
import { bumpSpecVersionAgainstNetwork } from './utils/bump_spec_version';
import { compileBinaries } from './utils/compile_binaries';
import { submitRuntimeUpgrade } from './submit_runtime_upgrade';

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

async function incompatibleUpgrade(
  currentVersionWorkspacePath: string,
  nextVersionWorkspacePath: string,
  numberOfNodes: 1 | 3,
) {
  await bumpSpecVersionAgainstNetwork(nextVersionWorkspacePath);

  await compileBinaries('all', nextVersionWorkspacePath);

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
    )}" LOCALNET_INIT_DIR=${currentVersionWorkspacePath}/localnet/init BINARY_ROOT_PATH=${nextVersionWorkspacePath}/target/release ${currentVersionWorkspacePath}/localnet/init/scripts/start-all-engines.sh`,
  );

  // let the engines do what they gotta do
  sleep(6000);

  console.log('Engines started');

  await submitRuntimeUpgrade(nextVersionWorkspacePath);

  console.log(
    'Check that the old engine has now shut down, and that the new engine is now running.',
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

  // For some reason shit isn't working here. Keeps thinking it's compatible or something.
  if (isCompatible) {
    // The CFE could be upgraded too. But an incompatible CFE upgrade would mean it's... incompatible, so covered in the other path.
    console.log('The versions are compatible.');

    await simpleRuntimeUpgrade(nextVersionWorkspacePath);
    console.log('Upgrade complete.');
  } else if (!isCompatible) {
    console.log('The versions are incompatible.');
    await incompatibleUpgrade(currentVersionWorkspacePath, nextVersionWorkspacePath, numberOfNodes);
  }

  console.log('Cleaning up...');
  execSync(`cd ${nextVersionWorkspacePath} && git worktree remove . --force`);
  console.log('Done.');
}
