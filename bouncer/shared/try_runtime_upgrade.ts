// This requires the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import path from 'path';
import { compileBinaries } from './utils/compile_binaries';
import { mkTmpDir, execWithRustLog } from './utils/exec_with_log';
import { getChainflipApi } from './utils/substrate';

async function createSnapshotFile(networkUrl: string, blockHash: string): Promise<boolean> {
  const blockParam = blockHash === 'latest' ? '' : `--at ${blockHash}`;
  const snapshotFolder = await mkTmpDir('chainflip/snapshots/');
  const snapshotOutputPath = path.join(snapshotFolder, `snapshot-at-${blockHash}.snap`);

  console.log('Writing snapshot to: ', snapshotOutputPath);

  return execWithRustLog(
    `try-runtime create-snapshot ${blockParam} --uri ${networkUrl} ${snapshotOutputPath}`,
    `create-snapshot-${blockHash}`,
    'runtime::executive=debug',
  );
}

async function tryRuntimeCommand(runtimePath: string, blockHash: 'latest' | string, networkUrl: string): Promise<boolean> {
  const blockParam = blockHash === 'latest' ? 'live' : `live --at ${blockHash}`;

  const success = await execWithRustLog(
    `try-runtime \
        --runtime ${runtimePath} on-runtime-upgrade \
        --disable-spec-version-check \
        --checks all ${blockParam} \
        --uri ${networkUrl}`,
    `try-runtime-${blockHash}`,
    'runtime::executive=debug',
  );
  if (!success) {
    await createSnapshotFile(networkUrl, blockHash);
  }
  return success;
}

// 4 options:
// - Live chain,
// - Specific block
// - All - goes from block 0 to the latest block when the script was started - this is useful for testing the upgrade on a local chain.
// - last-n, must also specify a number of blocks. This goes backwards from the latest block, running the migration on each block down the chain.
export async function tryRuntimeUpgrade(
  block: number | 'latest' | 'all' | 'last-n',
  networkUrl: string,
  runtimePath: string,
  lastN = 50,
) {
  await using api = await getChainflipApi();

  if (block === 'all') {
    const latestBlock = await api.rpc.chain.getBlockHash();

    console.log('Running migrations until we reach block with hash: ' + latestBlock);

    let blockNumber = 1;
    let blockHash = await api.rpc.chain.getBlockHash(blockNumber);
    while (!blockHash.eq(latestBlock)) {
      blockHash = await api.rpc.chain.getBlockHash(blockNumber);
      await tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl);
      blockNumber++;
    }
    console.log(`Block ${latestBlock} has been reached, exiting.`);
  } else if (block === 'last-n') {
    console.log(`Running migrations for the last ${lastN} blocks.`);
    let blocksProcessed = 0;

    let nextHash = await api.rpc.chain.getBlockHash();

    while (blocksProcessed < lastN) {
      await tryRuntimeCommand(runtimePath, `${nextHash}`, networkUrl);

      const currentBlockHeader = await api.rpc.chain.getHeader(nextHash);
      nextHash = currentBlockHeader.parentHash;
      blocksProcessed++;
    }
  } else if (block === 'latest') {
    await tryRuntimeCommand(runtimePath, 'latest', networkUrl);
  } else {
    const blockHash = await api.rpc.chain.getBlockHash(block);
    await tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl);
  }

  console.log('try-runtime upgrade successful.');
}

export async function tryRuntimeUpgradeWithCompileRuntime(
  block: number | 'latest' | 'all' | 'last-n',
  projectRoot: string,
  networkUrl: string,
  lastN = 50,
) {
  await compileBinaries('runtime', projectRoot);
  await tryRuntimeUpgrade(
    block,
    networkUrl,
    `${projectRoot}/target/release/wbuild/state-chain-runtime/state_chain_runtime.compact.compressed.wasm`,
    lastN,
  );
}
