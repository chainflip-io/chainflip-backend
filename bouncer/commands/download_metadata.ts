#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Downloads substrate metadata (version 15) from a Chainflip node.
//
// Arguments:
//   --rpc-url <url>       RPC endpoint (default: https://mainnet-archive.chainflip.io)
//   --runtime-version <n> Target runtime spec_version (default: current version)
//   --output <path>       Output file path (default: ../state-chain/runtime_historical_metadata/runtime_{VERSION}_{BLOCKHASH}.scale)
//
// Examples:
//   ./commands/download_metadata.ts
//   ./commands/download_metadata.ts --runtime-version 20100
//   ./commands/download_metadata.ts --rpc-url http://localhost:9944 --runtime-version 20000

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { parseArgs } from 'util';
import { Option, Bytes } from 'scale-ts';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const { values } = parseArgs({
    options: {
        'rpc-url': { type: 'string', default: 'https://mainnet-archive.chainflip.io' },
        'runtime-version': { type: 'string' },
        output: { type: 'string' },
    },
});

const rpcUrl = values['rpc-url']!;

let rpcId = 0;

async function rpc(method: string, params: unknown[] = []): Promise<unknown> {
    rpcId++;
    const response = await fetch(rpcUrl, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', method, params, id: rpcId }),
    });
    const data = await response.json();
    if (data.error) {
        throw new Error(`RPC error (${method}): ${data.error.message}`);
    }
    return data.result;
}

async function getBlockHash(blockNumber: number): Promise<string> {
    return (await rpc('chain_getBlockHash', [blockNumber])) as string;
}

async function getLatestBlockNumber(): Promise<number> {
    const header = (await rpc('chain_getHeader')) as { number: string };
    return parseInt(header.number, 16);
}

async function getRuntimeVersionAt(blockHash: string): Promise<number> {
    const version = (await rpc('state_getRuntimeVersion', [blockHash])) as { specVersion: number };
    return version.specVersion;
}

// Binary search for a block where the target runtime version is active.
// Returns the block hash of a block running the target version,
// or null if the version was never active on this chain.
async function findBlockWithVersion(targetVersion: number): Promise<string | null> {
    const latestBlock = await getLatestBlockNumber();
    let low = 0;
    let high = latestBlock;

    // First check: is the target version within range?
    const genesisHash = await getBlockHash(0);
    const genesisVersion = await getRuntimeVersionAt(genesisHash);

    const latestHash = await getBlockHash(latestBlock);
    const latestVersion = await getRuntimeVersionAt(latestHash);

    if (targetVersion < genesisVersion || targetVersion > latestVersion) {
        return null;
    }

    if (latestVersion === targetVersion) {
        return latestHash;
    }

    // Binary search: find any block with the target runtime version.
    // Runtime versions are monotonically increasing, so we search for the
    // transition point where specVersion changes to our target.
    let step = 0;
    while (low <= high) {
        const mid = Math.floor((low + high) / 2);
        const midHash = await getBlockHash(mid);
        const midVersion = await getRuntimeVersionAt(midHash);
        step++;
        console.log(`  [step ${step}] block #${mid} has version ${midVersion} (search range: ${low}..${high})`);

        if (midVersion === targetVersion) {
            console.log(`Found target version ${targetVersion} at block #${mid}`);
            return midHash;
        } else if (midVersion < targetVersion) {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }

    return null;
}

async function main() {
    console.log(`RPC endpoint: ${rpcUrl}`);

    // Determine target version
    let targetVersion: number;
    if (values['runtime-version']) {
        targetVersion = parseInt(values['runtime-version'], 10);
        if (isNaN(targetVersion)) {
            console.error('Error: --runtime-version must be a number');
            process.exit(1);
        }
    } else {
        const latestHash = (await rpc('chain_getHeader')) as { number: string };
        const hash = await getBlockHash(parseInt(latestHash.number, 16));
        targetVersion = await getRuntimeVersionAt(hash);
        console.log(`Using current runtime version: ${targetVersion}`);
    }

    // Find a block with the target version
    let blockHash: string;
    const latestBlock = await getLatestBlockNumber();
    const latestHash = await getBlockHash(latestBlock);
    const currentVersion = await getRuntimeVersionAt(latestHash);

    if (currentVersion === targetVersion) {
        console.log(`Target version ${targetVersion} is the current version.`);
        blockHash = latestHash;
    } else {
        console.log(
            `Target version ${targetVersion} differs from current (${currentVersion}). Searching...`,
        );
        const found = await findBlockWithVersion(targetVersion);
        if (!found) {
            console.error(
                `Error: Runtime version ${targetVersion} was not found on this chain.`,
            );
            process.exit(1);
        }
        blockHash = found;
    }

    // Download metadata v15 using state_call at the target block
    console.log(`Downloading metadata v15 at block hash ${blockHash}...`);

    // Call Metadata_metadata_at_version runtime API with version 15.
    // Parameter is the SCALE-encoded u32 value of 15 (little-endian: 0f000000).
    const rawResult = (await rpc('state_call', [
        'Metadata_metadata_at_version',
        '0x0f000000',
        blockHash,
    ])) as string;

    if (!rawResult || rawResult === '0x') {
        console.error('Error: Metadata v15 not available at this block (runtime may be too old).');
        process.exit(1);
    }

    // Decode the SCALE-encoded Option<OpaqueMetadata> (i.e. Option<Vec<u8>>)
    const hex = rawResult.startsWith('0x') ? rawResult.slice(2) : rawResult;
    const opaqueMetadata = Option(Bytes()).dec(Buffer.from(hex, 'hex'));

    if (opaqueMetadata === undefined) {
        console.error('Error: Metadata v15 not available at this block (returned None).');
        process.exit(1);
    }

    const metadata = Buffer.from(opaqueMetadata);
    const outputPath = values['output'] ??
        path.join(__dirname, '..', '..', 'state-chain', 'runtime_historical_metadata', `runtime_${targetVersion}.scale`);
    fs.mkdirSync(path.dirname(outputPath), { recursive: true });
    fs.writeFileSync(outputPath, metadata);
    console.log(`Metadata v15 (${metadata.length} bytes) written to ${outputPath}`);
}

main().catch((err) => {
    console.error(err);
    process.exit(1);
});
