import { execSync } from "child_process";

// Bumps the version of all the packages in the workspace by the specified level.
export async function bumpReleaseVersion(level: 'major' | 'minor' | 'patch') {
    try {
        execSync('cd ../ && cargo ws version ' + level + ' --no-git-commit -y')
    } catch (error) {
        console.log(error)
        console.log("Ensure you have cargo workspaces installed: `cargo install cargo-workspaces`")
    }
}
