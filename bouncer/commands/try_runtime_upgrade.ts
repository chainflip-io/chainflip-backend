#!/usr/bin/env -S pnpm tsx

// TODO: Document how to use the command.

import path from 'path';
import { tryRuntimeUpgrade } from "../shared/try_runtime_upgrade";
import { getChainflipApi, runWithTimeout } from "../shared/utils";
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';


async function main(): Promise<void> {
    const argv = yargs(hideBin(process.argv)).boolean('compile').default('compile', false).argv;


    const block = argv.block;

    if (block == undefined) {
        console.error('Must provide a block number to try the upgrade at. The options are to use a block number, or `latest` to use the latest block number on the network.');
        process.exit(-1);
    }

    const endpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
    let chainflipApi = await getChainflipApi();

    await tryRuntimeUpgrade(block, chainflipApi, endpoint, path.dirname(process.cwd()), argv.compile);

    process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
    console.error(error);
    process.exit(-1);
});
