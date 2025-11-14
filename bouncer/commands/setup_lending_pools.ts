#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes no arguments.
// This command will create lending pools for all supported assets and fund the BTC lending pool.

import { setupLendingPools } from 'shared/lending';
import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';

await runWithTimeoutAndExit(setupLendingPools(globalLogger), 120);
