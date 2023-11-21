// This requirse the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import { ApiPromise } from "@polkadot/api";
import { execSync } from "child_process";
import { compileBinaries } from "./utils/compile_binaries";



// Either we perform one at the head of a live chain, or we perform one at a particular block number.
export async function tryRuntimeUpgrade(block: number | 'latest', api: ApiPromise, networkUrl: string, projectRoot: string, shouldCompile: boolean = true) {

    if (shouldCompile) {
        compileBinaries('runtime', projectRoot);
    } else {
        console.log("Using pre-compiled state chain runtime for try-runtime upgrade.")
    }

    let blockParam;
    if (block == 'latest') {
        blockParam = 'live'
    } else {
        const blockHash = await api.rpc.chain.getBlockHash(block);
        blockParam = `live --at ${blockHash}`;
    }
    execSync(`try-runtime --runtime ${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm on-runtime-upgrade --checks all ${blockParam} --uri ${networkUrl}`);
}
