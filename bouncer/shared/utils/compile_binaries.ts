import { execSync } from "child_process";


// Returns the expected next version of the runtime.
export async function compileBinaries(type: "runtime" | "all", projectRoot: string) {
    if (type === "all") {
        console.log('Building all the binaries...');
        execSync(`cd ${projectRoot} cargo build --release`);
    } else {
        console.log('Building the new runtime...');
        execSync(`cd ${projectRoot}/state-chain/runtime && cargo build --release`);
    }

    console.log("Build complete.");
}