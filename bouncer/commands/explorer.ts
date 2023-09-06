#!/usr/bin/env -S pnpm tsx

// Set the node you want to query by setting the `CF_NODE_ENDPOINT` environment variable.
// e.g. CF_NODE_ENDPOINT=wss://perseverance.chainflip.xyz
// Call with a range of blocks to query, like:
// ./explorer.js 1234 1300
// Alternatively, the first argument can be the string "live" to query the latest blocks.
// In that case, the second argument specifying the last block to report is optional:
// ./explorer.ts live
//    or
// ./explorer.ts live 5000
//
// It can be convenient to filter out annoying events using grep, for example:
// ./explorer.ts live | grep -Fv -e "ExtrinsicSuccess" -e "update_chain_state" -e "TransactionFeePaid" -e "heartbeat" -e "timestamp"
// or you can show only lines that contain one of your specified words:
// ./explorer.ts live | grep -F -e "Block" -e "ChainStateUpdated"

import { ApiPromise } from '@polkadot/api';
import { getChainflipApi } from '../shared/utils';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function argsToString(args: any): string {
  return (
    args
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      .map((a: any) => {
        if (a.meta !== undefined && a.args !== undefined) {
          return `${a.section.toString()}.${a.meta.name.toString()}(${argsToString(a.args)})`;
        }
        return a.toString();
      })
      .join(', ')
  );
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function printExtrinsic(ex: any) {
  const {
    method: { args, method, section },
  } = ex.extrinsic;
  console.log(`  Extrinsic ${ex.block}-${ex.index}: ${section}.${method}(${argsToString(args)})`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function printEventsForExtrinsicId(events: any, extrinsicId: number, blockNum: number) {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  events.forEach((event: any, i: number) => {
    const decodedEvent = event.toHuman()!;
    if (
      Number(decodedEvent.phase.ApplyExtrinsic) === Number(extrinsicId) ||
      (extrinsicId === -1 &&
        (decodedEvent.phase === 'Initialization' || decodedEvent.phase === 'Finalization'))
    ) {
      const tag = extrinsicId === -1 ? decodedEvent.phase : extrinsicId;
      console.log(
        `    Event ${blockNum}-${tag}-${i}: ${decodedEvent.event.section}.${
          decodedEvent.event.method
        }(${argsToString(event.event.data)})`,
      );
    }
  });
}

async function processBlock(blockNumber: number, api: ApiPromise) {
  const blockHash = await api.rpc.chain.getBlockHash(blockNumber);
  const signedBlock = await api.rpc.chain.getBlock(blockHash);
  const events = await (await api.at(blockHash)).query.system.events();
  console.log();
  console.log(`Block ${blockNumber}:`);
  printEventsForExtrinsicId(events, -1, blockNumber);
  signedBlock.block.extrinsics.forEach((ex, i) => {
    printExtrinsic({ block: blockNumber, index: i, extrinsic: ex });
    printEventsForExtrinsicId(events, i, blockNumber);
  });
}

async function main() {
  const start = process.argv[2];
  const end = process.argv[3];
  const api = await getChainflipApi();
  if (start === 'live') {
    const unsubscribe = await api.rpc.chain.subscribeNewHeads(async (header) => {
      if (end && parseInt(end) < header.number.toNumber()) {
        unsubscribe();
        process.exit(0);
      }
      await processBlock(header.number.toNumber(), api);
    });
  } else {
    for (let blocknum = parseInt(start); blocknum <= parseInt(end); blocknum++) {
      await processBlock(blocknum, api);
    }
    process.exit(0);
  }
}

main();
