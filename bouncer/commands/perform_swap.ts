#!/usr/bin/env -S pnpm tsx
import { InternalAsset } from '@chainflip/cli';
import { performSwap } from 'shared/perform_swap';
import { parseAssetString } from 'shared/utils';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { globalLogger } from 'shared/utils/logger';

async function main() {
  const srcCcy = parseAssetString(process.argv[2]);
  const dstCcy = parseAssetString(process.argv[3]);
  const address = process.argv[4];
  await performSwap(
    await newChainflipIO(globalLogger, [] as []),
    srcCcy as InternalAsset,
    dstCcy as InternalAsset,
    address,
  );
  process.exit(0);
}

await main();
