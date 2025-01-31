#!/usr/bin/env -S pnpm tsx
import { InternalAsset } from '@chainflip/cli';
import { performSwap } from '../shared/perform_swap';
import { parseAssetString } from '../shared/utils';

async function main() {
  const srcCcy = parseAssetString(process.argv[2]);
  const dstCcy = parseAssetString(process.argv[3]);
  const address = process.argv[4];
  await performSwap(srcCcy as InternalAsset, dstCcy as InternalAsset, address, undefined);
  process.exit(0);
}

await main();
