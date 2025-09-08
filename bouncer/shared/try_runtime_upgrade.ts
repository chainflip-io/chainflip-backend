// This requires the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import path from 'path';
import { ApiPromise, HttpProvider } from '@polkadot/api';
import { compileBinaries } from 'shared/utils/compile_binaries';
import { mkTmpDir, execWithRustLog } from 'shared/utils/exec_with_log';
import { CHAINFLIP_HTTP_ENDPOINT } from 'shared/utils/substrate';
import { retryRpcCall } from 'shared/utils';
import { globalLogger as logger } from 'shared/utils/logger';

async function createSnapshotFile(networkUrl: string, blockHash: string): Promise<boolean> {
  const blockParam = blockHash === 'latest' ? '' : `--at ${blockHash}`;
  const snapshotFolder = await mkTmpDir('chainflip/snapshots/');
  const snapshotOutputPath = path.join(snapshotFolder, `snapshot-at-${blockHash}.snap`);

  logger.info('Writing snapshot to: ', snapshotOutputPath);

  return execWithRustLog(
    `try-runtime`,
    `create-snapshot ${blockParam} --uri ${networkUrl} ${snapshotOutputPath}`.split(' '),
    `create-snapshot-${blockHash}`,
    'info,runtime::executive=debug',
  );
}

async function tryRuntimeCommand(
  runtimePath: string,
  blockHash: 'latest' | string,
  networkUrl: string,
): Promise<boolean> {
  const blockParam = blockHash === 'latest' ? 'live' : `live --at ${blockHash}`;

  // NOTE: the `--disable-mbm-checks` flag is very important:
  // On version 0.8.0 of try-runtime, there's a bug where without this flag,
  // the pre-and-post checks are run on a state which is already migrated.
  // This means that it will not catch any migration issues which only manifest themselves
  // when running on a pre-migrated state.
  const success = await execWithRustLog(
    `try-runtime`,
    `\
--runtime ${runtimePath} on-runtime-upgrade \
--blocktime 6000 \
--disable-spec-version-check \
--checks all ${blockParam} \
--disable-mbm-checks \
--uri ${networkUrl}`.split(' '),
    `try-runtime-${blockHash}`,
    'info,runtime::executive=debug',
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
  lastN = 20,
) {
  const httpApi = await ApiPromise.create({
    provider: new HttpProvider(CHAINFLIP_HTTP_ENDPOINT),
    noInitWarn: true,
  });

  if (block === 'all') {
    const latestBlock = await httpApi.rpc.chain.getBlockHash();

    logger.info('Running migrations until we reach block with hash: ' + latestBlock);

    let blockNumber = 1;
    let blockHash = await httpApi.rpc.chain.getBlockHash(blockNumber);
    while (!blockHash.eq(latestBlock)) {
      blockHash = await httpApi.rpc.chain.getBlockHash(blockNumber);
      const success = await tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl);

      if (!success) {
        throw new Error('Migration failed for block: ' + blockHash.toString());
      }

      blockNumber++;
    }
    logger.info(`Block ${latestBlock} has been reached, exiting.`);
  } else if (block === 'last-n') {
    logger.info(`Running migrations for the last ${lastN} blocks.`);
    let blocksProcessed = 0;

    let nextHash = await httpApi.rpc.chain.getBlockHash();

    while (blocksProcessed < lastN) {
      logger.info('Running try-runtime for block: ', nextHash.toString());

      const success = await tryRuntimeCommand(runtimePath, `${nextHash}`, networkUrl);

      if (!success) {
        throw new Error('Migration failed for block: ' + nextHash.toString());
      }

      const currentHash = nextHash;
      const currentBlockHeader = await retryRpcCall(
        () => httpApi.rpc.chain.getHeader(currentHash),
        {
          maxAttempts: 10,
          timeoutMs: 20000,
          operation: `get block header at ${currentHash}`,
        },
      );
      nextHash = currentBlockHeader.parentHash;

      blocksProcessed++;
    }
  } else if (block === 'latest') {
    const success = await tryRuntimeCommand(runtimePath, 'latest', networkUrl);
    if (!success) {
      throw new Error('Migration failed for latest block');
    }
  } else {
    const blockHash = await httpApi.rpc.chain.getBlockHash(block);
    const success = await tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl);
    if (!success) {
      throw new Error('Migration failed for latest block');
    }
  }

  logger.info('try-runtime upgrade successful.');
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
