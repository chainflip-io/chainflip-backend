import { execSync } from "child_process";

export type SemVerLevel = 'major' | 'minor' | 'patch';

// Bumps the version of all the packages in the workspace by the specified level.
export async function bumpReleaseVersion(level: SemVerLevel, projectRoot: string) {
    console.log(`Bumping the version of all packages in the workspace by ${level}...`);
    try {
        execSync(`cd ${projectRoot} && cargo ws version ${level} --no-git-commit -y`)
    } catch (error) {
        console.log(error)
        console.log("Ensure you have cargo workspaces installed: `cargo install cargo-workspaces`")
    }
}
