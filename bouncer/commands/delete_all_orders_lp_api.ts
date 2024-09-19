#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no argument,
// It calls the lp API cancel_all_orders which queries for all the open orders an LP has and then delete them all

import { executeWithTimeout } from '../shared/utils';
import { createAndDeleteAllOrdersLpApi } from '../shared/delete_all_orders_lp_api';

async function main(): Promise<void> {
  await createAndDeleteAllOrdersLpApi();
}

await executeWithTimeout(main(), 240);
