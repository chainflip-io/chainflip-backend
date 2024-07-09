#!/usr/bin/env -S pnpm tsx
import { SwapContext, testAllSwaps } from '../shared/swapping';
import { executeWithTimeout } from '../shared/utils';

const swapContext = new SwapContext();

async function main(): Promise<void> {
  await testAllSwaps(swapContext);
  swapContext.print_report();
}

await executeWithTimeout(main(), 600);
