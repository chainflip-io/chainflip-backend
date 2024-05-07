#!/usr/bin/env -S pnpm tsx
import { SwapContext, testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

const swapContext = new SwapContext();

async function main(): Promise<void> {
  await testAllSwaps(swapContext);
  swapContext.print_report();
  process.exit(0);
}

runWithTimeout(main(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    swapContext.print_report();
    process.exit(-1);
  });
