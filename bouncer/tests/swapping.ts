#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';
import { runWithTimeout } from '../shared/utils';

runWithTimeout(testAllSwaps(), 1800000)
  .then(() => {
    // there are some dangling resources that prevent the process from exiting
    process.exit(0);
  })
  .catch((error) => {
    console.error(error);
    process.exit(-1);
  });
