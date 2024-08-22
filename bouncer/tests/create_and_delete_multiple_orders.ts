#!/usr/bin/env -S pnpm tsx
import { executeWithTimeout } from '../shared/utils';
import { createAndDeleteMultipleOrders } from '../shared/create_and_delete_multiple_orders';

async function main(): Promise<void> {
  console.log('Testing close_orders_batch');

  await createAndDeleteMultipleOrders(25);
}

await executeWithTimeout(main(), 240);
