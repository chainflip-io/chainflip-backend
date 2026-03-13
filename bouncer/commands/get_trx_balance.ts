#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will print the TRX balance of the address provided as the first argument.
//
// For example: ./commands/get_trx_balance.ts TAeUa4FzpoveH7UUArvfXdeS3TmXiWU1ds
// or ./commands/get_trx_balance.ts 0x4c6dc6656b379c2ea87ab0758e06352f78236931
// might print: 100000 TRX

import { runWithTimeoutAndExit } from 'shared/utils';
import { getTrxBalance } from 'shared/get_trx_balance';

const address = process.argv[2];
if (!address) {
  console.error('Usage: get_trx_balance.ts <tron-address>');
  process.exit(1);
}

await runWithTimeoutAndExit(
  getTrxBalance(address).then((balance) => console.log(balance + ' TRX')),
  10,
);
