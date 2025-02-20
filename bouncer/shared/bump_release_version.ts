import { execSync } from 'child_process';
import { globalLogger } from './utils/logger';

export type SemVerLevel = 'major' | 'minor' | 'patch';

// Bumps the version of all the packages in the workspace by the specified level.
export async function bumpReleaseVersion(level: SemVerLevel, projectRoot: string) {
  globalLogger.info(`Bumping the version of all packages in the workspace by ${level}...`);
  try {
    execSync(`cd ${projectRoot} && cargo ws version ${level} --no-git-commit -y`);
  } catch (error) {
    globalLogger.error(error);
    globalLogger.warn(
      'Ensure you have cargo workspaces installed: `cargo install cargo-workspaces`',
    );
  }
}
