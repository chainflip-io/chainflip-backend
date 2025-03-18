#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no argument,
// It calls the lp API cancel_all_orders which queries for all the open orders an LP has and then delete them all

import { runWithTimeoutAndExit } from '../shared/utils';
import { DeleteAllOrdersLpApi } from '../shared/delete_all_orders_lp_api';
import { globalLogger } from '../shared/utils/logger';

async function main(): Promise<void> {
  await DeleteAllOrdersLpApi(globalLogger);
}

await runWithTimeoutAndExit(main(), 240);
