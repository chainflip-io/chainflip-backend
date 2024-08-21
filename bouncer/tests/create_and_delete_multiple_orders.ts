#!/usr/bin/env -S pnpm tsx
import { executeWithTimeout } from '../shared/utils';
import { createAndDeleteAllOrders } from '../shared/create_and_delete_all_open_orders';

await executeWithTimeout(createAndDeleteAllOrders(25), 240);

async function main(): Promise<void> {
  console.log('Testing close_orders_batch');

  await createAndDeleteAllOrders(25);
}

await executeWithTimeout(main(), 240);
