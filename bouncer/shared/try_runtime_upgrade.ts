// This requirse the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import { ApiPromise } from "@polkadot/api";
import { execSync } from "child_process";
import { compileBinaries } from "./utils/compile_binaries";


// 4 options:
// - Live chain, 
// - Specific block
// - All - goes from block 0 to the latest block when the script was started - this is useful for testing the upgrade on a local chain.
// - last-n, must also specify a number of blocks. This goes backwards from the latest block, running the migration on each block down the chain.
export async function tryRuntimeUpgrade(block: number | 'latest' | 'all' | 'last-n', api: ApiPromise, networkUrl: string, projectRoot: string, shouldCompile: boolean = true, lastN: number = 50) {

    if (shouldCompile) {
        compileBinaries('runtime', projectRoot);
    } else {
        console.log("Using pre-compiled state chain runtime for try-runtime upgrade.")
    }

    if (block == 'all') {
        const latestBlock = await api.rpc.chain.getBlockHash();

        console.log("Running migrations until we reach block with hash: " + latestBlock);

        let blockNumber = 1;
        while (true) {
            const blockHash = await api.rpc.chain.getBlockHash(blockNumber);

            try {
                tryRuntimeCommand(projectRoot, `live --at ${blockHash}`, networkUrl);
                console.log(`try-runtime success for block ${blockNumber}, block hash: ${blockHash}`);
            } catch (e) {
                console.error(`try-runtime failed for block ${blockNumber}, block hash: ${blockHash}`);
                console.error(e);
                process.exit(-1);
            }

            if (blockHash.eq(latestBlock)) {
                console.log(`Block ${latestBlock} has been reached, exiting.`);
                break
            }
            blockNumber++;
        }
    } else if (block == 'last-n') {
        console.log(`Running migrations for the last ${lastN} blocks.`);
        let blocksProcessed = 0;

        let nextHash = await api.rpc.chain.getBlockHash();

        while (blocksProcessed < lastN) {

            try {
                tryRuntimeCommand(projectRoot, `live --at ${nextHash}`, networkUrl);
                console.log(`try-runtime success for block hash: ${nextHash}`);
            } catch (e) {
                console.error(`try-runtime failed for block hash: ${nextHash}`);
                console.error(e);
                process.exit(-1);
            }

            let currentBlockHeader = await api.rpc.chain.getHeader(nextHash);
            nextHash = currentBlockHeader.parentHash;
            blocksProcessed++;
        }
    } else if (block == 'latest') {
        tryRuntimeCommand(projectRoot, 'live', networkUrl);
    } else {
        const blockHash = await api.rpc.chain.getBlockHash(block);
        tryRuntimeCommand(projectRoot, `live --at ${blockHash}`, networkUrl);
    }

    console.log("try-runtime upgrade successful.");
}

function tryRuntimeCommand(projectRoot: string, blockParam: string, networkUrl: string) {
    execSync(`try-runtime --runtime ${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.wasm on-runtime-upgrade --checks all ${blockParam} --uri ${networkUrl}`, { stdio: 'ignore' });
}