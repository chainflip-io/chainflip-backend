// This requires the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import os from 'os';
import path from 'path';
import { ApiPromise, HttpProvider } from '@polkadot/api';
import { compileBinaries } from 'shared/utils/compile_binaries';
import { mkTmpDir, execWithRustLog } from 'shared/utils/exec_with_log';
import { CHAINFLIP_HTTP_ENDPOINT } from 'shared/utils/substrate';
import { retryRpcCall } from 'shared/utils';
import { globalLogger as logger, loggerChild, type Logger } from 'shared/utils/logger';

// Returns the snapshot file path on success, or null on failure.
async function createSnapshotFile(
  networkUrl: string,
  blockHash: string,
  childLogger: Logger,
): Promise<string | null> {
  const blockParam = blockHash === 'latest' ? '' : `--at ${blockHash}`;
  const snapshotFolder = await mkTmpDir('chainflip/snapshots/');
  const snapshotOutputPath = path.join(snapshotFolder, `snapshot-at-${blockHash}.snap`);

  childLogger.info('Writing snapshot to: ', snapshotOutputPath);

  const success = await execWithRustLog(
    `try-runtime`,
    `create-snapshot ${blockParam} --uri ${networkUrl} ${snapshotOutputPath}`.split(' '),
    `create-snapshot-${blockHash}`,
    'info,runtime::executive=debug',
    childLogger,
  );
  return success ? snapshotOutputPath : null;
}

async function tryRuntimeCommandFromSnapshot(
  runtimePath: string,
  snapshotPath: string,
  childLogger: Logger = logger,
): Promise<boolean> {
  // NOTE: the `--disable-mbm-checks` flag is very important:
  // On version 0.8.0 of try-runtime, there's a bug where without this flag,
  // the pre-and-post checks are run on a state which is already migrated.
  // This means that it will not catch any migration issues which only manifest themselves
  // when running on a pre-migrated state.
  return execWithRustLog(
    `try-runtime`,
    `\
--runtime ${runtimePath} on-runtime-upgrade \
--blocktime 6000 \
--disable-spec-version-check \
--disable-mbm-checks \
--checks all snap \
--path ${snapshotPath}`.split(' '),
    `try-runtime-${path.basename(snapshotPath)}`,
    'info,runtime::executive=debug',
    childLogger,
  );
}

async function tryRuntimeCommand(
  runtimePath: string,
  blockHash: 'latest' | string,
  networkUrl: string,
  childLogger: Logger = logger,
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
--disable-mbm-checks \
--checks all ${blockParam} \
--uri ${networkUrl}`.split(' '),
    `try-runtime-${blockHash}`,
    'info,runtime::executive=debug',
    childLogger,
  );
  if (!success) {
    await createSnapshotFile(networkUrl, blockHash, childLogger);
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
        throw new Error(
          `Migration failed for block: ${blockHash.toString()} please check the full try-runtime log file for this block`,
        );
      }

      blockNumber++;
    }
    logger.info(`Block ${latestBlock} has been reached, exiting.`);
  } else if (block === 'last-n') {
    // Each try-runtime process is CPU-bound (wasm execution) so capping parallelism to the number
    // of logical CPUs avoids resource contention and OOM on memory-constrained CI runners.
    const BATCH_SIZE = os.cpus().length;
    logger.info(`Running migrations for the last ${lastN} blocks in batches of ${BATCH_SIZE}.`);

    // Phase 1: Fetch block hashes and create snapshot files sequentially.
    // Snapshot creation is RPC-bound and reconstructs remote state, so doing them one at a time
    // avoids overloading the node with concurrent large state fetches.
    type SnapshotEntry = { hash: string; snapshotPath: string };
    const snapshots: SnapshotEntry[] = [];
    let nextHash = await httpApi.rpc.chain.getBlockHash();
    for (let i = 0; i < lastN; i++) {
      const currentHash = nextHash;
      const hashStr = currentHash.toString();
      const blockLogger = loggerChild(logger, `block_${hashStr}`);
      blockLogger.info('Creating snapshot for block: ', hashStr);
      const snapshotPath = await createSnapshotFile(networkUrl, hashStr, blockLogger);
      if (snapshotPath === null) {
        throw new Error(`Failed to create snapshot for block ${hashStr}`);
      }
      snapshots.push({ hash: hashStr, snapshotPath });
      const header = await retryRpcCall(() => httpApi.rpc.chain.getHeader(currentHash), {
        maxAttempts: 10,
        timeoutMs: 20000,
        operation: `get block header at ${hashStr}`,
      });
      nextHash = header.parentHash;
    }

    // Phase 2: Run try-runtime from local snapshots in parallel batches (CPU-bound).
    for (let i = 0; i < snapshots.length; i += BATCH_SIZE) {
      const batch = snapshots.slice(i, i + BATCH_SIZE);
      const results = await Promise.all(
        batch.map(async ({ hash, snapshotPath }) => {
          const blockLogger = loggerChild(logger, `block_${hash}`);
          blockLogger.info('Running try-runtime for block: ', hash);
          const success = await tryRuntimeCommandFromSnapshot(runtimePath, snapshotPath, blockLogger);
          return { hash, success };
        }),
      );

      const failed = results.filter((r) => !r.success);
      if (failed.length > 0) {
        throw new Error(
          `Migration failed for blocks: ${failed.map((r) => r.hash).join(', ')} please check the full try-runtime log file for these blocks`,
        );
      }
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
      throw new Error(
        'Migration failed for latest block please check the full try-runtime log file for this block',
      );
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
