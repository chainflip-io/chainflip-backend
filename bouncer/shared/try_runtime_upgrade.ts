// This requires the try-runtime cli to be installed globally
// https://github.com/paritytech/try-runtime-cli

import path from 'path';
import fs from 'fs';
import { compileBinaries } from './utils/compile_binaries';
import { createTmpDirIfNotExists, execWithRustLog } from './utils/exec_with_log';
import { retryRpcCall } from './utils';
import { setTimeout as sleep } from 'timers/promises';
import { ApiPromise, HttpProvider } from '@polkadot/api';

// Return the path to the snapshot file
function createSnapshotFile(networkUrl: string, blockHash: string, failureObj: FailureObj | null) {
  const blockParam = blockHash === 'latest' ? '' : `--at ${blockHash}`;
  const snapshotFolder = createTmpDirIfNotExists('chainflip/snapshots/');
  const snapshotOutputPath = path.join(snapshotFolder, `snapshot-at-${blockHash}.snap`);

  console.log('Writing snapshot to: ', snapshotOutputPath);

  execWithRustLog(
    `try-runtime create-snapshot ${blockParam} --uri ${networkUrl} ${snapshotOutputPath}`,
    `create-snapshot-${blockHash}`,
    'runtime::executive=debug',
    (success) => {
      if (!success) {
        console.error('Failed to create snapshot.');
        process.exitCode = 1;
      } else {
        if (failureObj) {
          failureObj.snapshotPath = snapshotOutputPath;
        }
      }
    },
  );
}

async function tryRuntimeCommand(runtimePath: string, blockHash: 'latest' | string, networkUrl: string, failureObj: FailureObj) {
  const blockParam = blockHash === 'latest' ? 'live' : `live --at ${blockHash}`;

    let exitCode = 0;
  
    execWithRustLog(
      `try-runtime \
          --runtime ${runtimePath} on-runtime-upgrade \
          --blocktime 6000 \
          --disable-spec-version-check \
          --checks all ${blockParam} \
          --uri ${networkUrl}`,
      `try-runtime-${blockHash}`,
      'runtime::executive=debug',
      (success, logFile) => {
        if (!success) {
          const logContents = fs.readFileSync(logFile, 'utf8');
          console.error(logContents);
          exitCode = 1;
          failureObj.hash = blockHash;
        }
      },
    );
}

type FailureObj = {
  hash: string | null,
  snapshotPath: string | null,
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
  lastN = 30,
) {

  const httpNetworkUrl = networkUrl.replace(/^wss?:/, (match) => match === 'wss:' ? 'https:' : 'http:');
  console.log(`Creating HTTP API for network URL: ${httpNetworkUrl}`);

  const httpApi = await ApiPromise.create({
    provider: new HttpProvider(httpNetworkUrl),
    noInitWarn: true,
  });

  // This is a placeholder object that will be used to store the failure object.
  // We use an object in order to pass by reference to the function.
  let failureObj: FailureObj = {
    hash: null,
    snapshotPath: null,
  }

  if (block === 'all') {
    const latestBlock = await httpApi.rpc.chain.getBlockHash();

    console.log('Running migrations until we reach block with hash: ' + latestBlock);

    let blockNumber = 1;
    let blockHash = await httpApi.rpc.chain.getBlockHash(blockNumber);
    while (!blockHash.eq(latestBlock)) {
      blockHash = await httpApi.rpc.chain.getBlockHash(blockNumber);
      tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl, failureObj);
      blockNumber++;
    }
    console.log(`Block ${latestBlock} has been reached, exiting.`);
  } else if (block === 'last-n') {
    console.log(`Running migrations for the last ${lastN} blocks.`);
    let blocksProcessed = 0;


    let nextHash = await httpApi.rpc.chain.getBlockHash();

    console.log('first nextHash: ', nextHash.toString());

    while (blocksProcessed < lastN) {
      tryRuntimeCommand(runtimePath, `${nextHash}`, networkUrl, failureObj);

      // Give the node some breathing time after working hard doing the try-runtime
      await sleep(2000);

      const currentBlockHeader = await retryRpcCall(() => httpApi.rpc.chain.getHeader(nextHash), {
        maxAttempts: 10,
        timeoutMs: 20000,
        operation: `get block header at ${nextHash}`,
      });
      nextHash = currentBlockHeader.parentHash.toString();
      console.log('nextHash: ', nextHash);

      if (failureObj.hash) {
        console.log("Creating snapshot in finally");
        createSnapshotFile(networkUrl, failureObj.hash, failureObj);
        if (failureObj.snapshotPath) {
          console.log('Snapshot created at: ', failureObj.snapshotPath);
          throw new Error('Snapshot created. Exiting.');
        } else {
          console.log('Snapshot not created yet...');
        }
      }

      blocksProcessed++;
    }
  } else if (block === 'latest') {
    tryRuntimeCommand(runtimePath, 'latest', networkUrl, failureObj);
  } else {
    const blockHash = await httpApi.rpc.chain.getBlockHash(block);
    tryRuntimeCommand(runtimePath, `${blockHash}`, networkUrl, failureObj);
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
