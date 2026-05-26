#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// Performs a single end-to-end swap against a running localnet: opens a deposit
// channel, sends the deposit, and waits for the swap to be witnessed, executed,
// and egressed. Useful for generating real swap activity (e.g. to inspect in the
// indexer) without running a full test.
//
// Arguments:
//   1. source asset   (e.g. Eth, Btc, Sol, ArbUsdc)
//   2. destination asset
//   3. destination address (optional) — if omitted, a fresh address for the
//      destination asset is generated automatically.
//
// Examples:
//   ./commands/perform_swap.ts Eth Usdc
//   ./commands/perform_swap.ts Btc Eth 0x6451c1113402D0ddf20D328d57d9D66dAa756b7a

import { performSwap } from 'shared/perform_swap';
import { parseAssetString, Asset, newAssetAddress } from 'shared/utils';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';

async function main() {
  const srcCcy = parseAssetString(process.argv[2]) as Asset;
  const dstCcy = parseAssetString(process.argv[3]) as Asset;
  const address = process.argv[4] ?? (await newAssetAddress(dstCcy));
  if (!process.argv[4]) {
    globalLogger.info(`No destination address given — generated ${dstCcy} address ${address}`);
  }
  await performSwap(await newChainflipIO(globalLogger, [] as []), srcCcy, dstCcy, address);
  process.exit(0);
}

await main();
