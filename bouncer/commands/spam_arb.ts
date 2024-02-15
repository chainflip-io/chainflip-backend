#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// It will send transactions to Arbitrum to have cointinous block production.

import { runWithTimeout } from '../shared/utils';
import { spamEvm } from '../shared/send_evm';

async function main() {
  // For now we just do every 6 sec
  await spamEvm('Arbitrum', 3000);

  process.exit(0);
}

runWithTimeout(main(), 200000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
