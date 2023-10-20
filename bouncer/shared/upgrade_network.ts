import { execSync } from 'child_process';
import fs from 'fs/promises';
import * as toml from 'toml';
import path from 'path';
import { promptUser } from './prompt_user';
import { SemVerLevel, bumpReleaseVersion } from './bump_release_version';
import { simpleRuntimeUpgrade } from './simple_runtime_upgrade';
import { compareSemVer } from './utils';

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

// Upgrades a bouncer network from the commit currently running on localnet to the provided git reference (commit, branch, tag).
// If the version of the commit we're upgrading to is the same as the version of the commit we're upgrading from, we bump the version by the specified level.
export async function upgradeNetwork(toGitRef: string, bumpByIfEqual: SemVerLevel = 'patch') {
  const fromTomlVersion = await readPackageTomlVersion(path.dirname(process.cwd()));
  console.log("Version we're upgrading from: " + fromTomlVersion);

  // abcd124 ensures there's no bracnh with the same name that will stop us from creating the workspace.
  // tmp/ is ignored in the bouncer .gitignore file.
  const absoluteWorkspacePath = path.join(process.cwd(), 'tmp/upgrade-network');

  console.log('Creating a new git workspace at: ' + absoluteWorkspacePath);

  createGitWorkspaceAt(absoluteWorkspacePath, toGitRef);

  const toTomlVersion = await readPackageTomlVersion(`${absoluteWorkspacePath}`);
  console.log("Version we're upgrading to: " + toTomlVersion);

  if (compareSemVer(fromTomlVersion, toTomlVersion) === 'greater') {
    throw new Error(
      "The version we're upgrading to is older than the version we're upgrading from. Ensure you selected the correct commits.",
    );
  }

  if (fromTomlVersion === toTomlVersion) {
    await bumpReleaseVersion(bumpByIfEqual, absoluteWorkspacePath);
  }

  const newToTomlVersion = await readPackageTomlVersion(path.join(absoluteWorkspacePath));
  const isCompatible = isCompatibleWith(fromTomlVersion, newToTomlVersion);

  if (isCompatible) {
    // The CFE could be upgraded too. But an incompatible CFE upgrade would mean it's... incompatible, so covered in the other path.
    console.log('The versions are compatible.');

    // Runtime upgrade using the *new* version.
    await simpleRuntimeUpgrade(absoluteWorkspacePath);
    console.log('Upgrade complete.');
  } else if (!isCompatible) {
    // Incompatible upgrades requires running two versions of the CFEs side by side.
    console.log('Incompatible CFE upgrades are not yet supported :(');
  }

  execSync(`cd ${absoluteWorkspacePath} && git worktree remove . --force`);
}

// Create a git workspace in the tmp/ directory and check out the specified commit.
// Remember to delete it when you're done!
function createGitWorkspaceAt(absoluteWorkspacePath: string, toGitRef: string) {
  try {
    // Create a directory for the new workspace
    execSync(`mkdir -p ${absoluteWorkspacePath}`);

    // Create a new workspace using git worktree.
    execSync(`git worktree add ${absoluteWorkspacePath}`);

    // Navigate to the new workspace and checkout the specific commit
    execSync(`cd ${absoluteWorkspacePath} && git checkout ${toGitRef}`);

    console.log('Commit checked out successfully in new workspace.');
  } catch (error) {
    console.error(`Error: ${error}`);
  }
}
