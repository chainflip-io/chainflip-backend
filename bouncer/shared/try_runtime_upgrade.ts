// This requires the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import { ApiPromise } from '@polkadot/api';
import { execSync } from 'child_process';
import path from 'path';
import os from 'os';
import fs from 'fs';
import { compileBinaries } from './utils/compile_binaries';

function createTmpDirIfNotExists(dir: string): string {
  const tmpDir = path.join(os.tmpdir(), dir);
  try {
    if (!fs.existsSync(tmpDir)) {
      fs.mkdirSync(tmpDir);
    }
  } catch (err) {
    console.error(err);
  }

  return tmpDir;
}

function createSnapshotFile(networkUrl: string, blockHash: string) {
  const snapshotFolder = createTmpDirIfNotExists('chainflip/snapshots/');
  const snapshotOutputPath = path.join(snapshotFolder, `snapshot-at-${blockHash}.snap`);
  const blockParam = blockHash === 'latest' ? '' : `--at ${blockHash}`;

  console.log('Writing snapshot to: ', snapshotOutputPath);

  try {
    execSync(
      `try-runtime create-snapshot ${blockParam} --uri ${networkUrl} ${snapshotOutputPath}`,
      { env: { ...process.env, RUST_LOG: 'runtime::executive=debug' }, stdio: [0, 1, 1] },
    );
  } catch (e) {
    console.error(`try-runtime create-snapshot failed.`);
  }
}

function tryRuntimeCommand(runtimePath: string, blockHash: 'latest' | string, networkUrl: string) {
  const blockParam = blockHash === 'latest' ? 'live' : `live --at ${blockHash}`;

  try {
    execSync(
      // TODO: Replace pre-and-post with all after the SDK issue paritytech/polkadot-sdk#2560 is merged.
      `try-runtime \
        --runtime ${runtimePath} on-runtime-upgrade \
        --disable-spec-version-check \
        --disable-idempotency-checks \
        --checks pre-and-post ${blockParam} \
        --uri ${networkUrl}`,
      { env: { ...process.env, RUST_LOG: 'runtime::executive=debug' }, stdio: [0, 1, 1] },
    );
    console.log(`try-runtime success for blockParam ${blockParam}`);
  } catch (e) {
    console.error(`try-runtime failed for blockParam ${blockParam}`);

    createSnapshotFile(networkUrl, blockHash);

    process.exit(-1);
  }
}

// 4 options:
// - Live chain,
// - Specific block
// - All - goes from block 0 to the latest block when the script was started - this is useful for testing the upgrade on a local chain.
// - last-n, must also specify a number of blocks. This goes backwards from the latest block, running the migration on each block down the chain.
export async function tryRuntimeUpgrade(
  block: number | 'latest' | 'all' | 'last-n',
  api: ApiPromise,
  networkUrl: string,
  runtimePath: string,
  lastN = 50,
) {
  if (block === 'all') {
    const latestBlock = await api.rpc.chain.getBlockHash();

    console.log('Running migrations until we reach block with hash: ' + latestBlock);

    let blockNumber = 1;
    let blockHash = await api.rpc.chain.getBlockHash(blockNumber);
    while (!blockHash.eq(latestBlock)) {
      blockHash = await api.rpc.chain.getBlockHash(blockNumber);
      tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl);

      blockNumber++;
    }
    console.log(`Block ${latestBlock} has been reached, exiting.`);
  } else if (block === 'last-n') {
    console.log(`Running migrations for the last ${lastN} blocks.`);
    let blocksProcessed = 0;

    let nextHash = await api.rpc.chain.getBlockHash();

    while (blocksProcessed < lastN) {
      tryRuntimeCommand(runtimePath, `${nextHash}`, networkUrl);

      const currentBlockHeader = await api.rpc.chain.getHeader(nextHash);
      nextHash = currentBlockHeader.parentHash;
      blocksProcessed++;
    }
  } else if (block === 'latest') {
    tryRuntimeCommand(runtimePath, 'latest', networkUrl);
  } else {
    const blockHash = await api.rpc.chain.getBlockHash(block);
    tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl);
  }

  console.log('try-runtime upgrade successful.');
}

export async function tryRuntimeUpgradeWithCompileRuntime(
  block: number | 'latest' | 'all' | 'last-n',
  api: ApiPromise,
  projectRoot: string,
  networkUrl: string,
  lastN = 50,
) {
  await compileBinaries('runtime', projectRoot);
  await tryRuntimeUpgrade(
    block,
    api,
    networkUrl,
    `${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
    lastN,
  );
}
