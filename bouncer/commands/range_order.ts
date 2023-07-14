// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: pnpm tsx ./commands/range_order.ts btc 10

import { Asset } from '@chainflip-io/cli';
import { rangeOrder } from '../shared/range_order';
import { runWithTimeout } from '../shared/utils';

async function main() {
  const ccy = process.argv[2].toUpperCase() as Asset;
  const amount = parseFloat(process.argv[3].trim());
  await rangeOrder(ccy, amount);
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
