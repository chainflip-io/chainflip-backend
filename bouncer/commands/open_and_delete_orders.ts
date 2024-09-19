#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument, the number of orders to create
// NB! we can delete a max of 100 orders simultaneously, using a value bigger than that will result in the extrinsic failing
// Moreover this command deletes the open_range_orders as well hence be sure to select a proper value for which the sum of the already open orders + the newly created doesn't exceed 100
// For example: ./commands/open_and_delete_orders.ts 5
// will create 5 limit_order and then delete them all with a single extrinsic

import { runWithTimeoutAndExit } from '../shared/utils';
import { createAndDeleteMultipleOrders } from '../tests/create_and_delete_multiple_orders';

async function main(): Promise<void> {
  if (!process.argv[2]) {
    console.log('Number of orders not provided!');
    process.exit(-1);
  }
  const numberOfOrders = process.argv[2];

  await createAndDeleteMultipleOrders(Number(numberOfOrders));
}

await runWithTimeoutAndExit(main(), 240);
