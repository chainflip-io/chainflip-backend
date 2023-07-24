#!/usr/bin/env -S pnpm tsx
import { Asset } from '@chainflip-io/cli';
import { performSwap } from '../shared/perform_swap';

async function main() {
  const srcCcy = process.argv[2].toUpperCase() as Asset;
  const dstCcy = process.argv[3].toUpperCase() as Asset;
  const address = process.argv[4];
  await performSwap(srcCcy, dstCcy, address);
  process.exit(0);
}

main();
