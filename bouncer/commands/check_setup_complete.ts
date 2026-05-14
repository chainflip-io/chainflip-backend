#!/usr/bin/env -S pnpm tsx
// Checks whether ./bouncer/setup_for_test.sh has been run against the current localnet.
//
// Usage (from bouncer/):
//   ./commands/check_setup_complete.ts
//
// Exit codes:
//   0 — setup has run (BTC lending pool exists)
//   1 — setup has not run, or localnet not reachable

import { getChainflipApi } from 'shared/utils/substrate';
import { globalLogger as logger } from 'shared/utils/logger';

async function main() {
  const api = await getChainflipApi();
  const btcPool = (await api.query.lendingPools.generalLendingPools('Btc')).toJSON();
  if (btcPool === null) {
    logger.info('NOT_SET_UP');
    process.exit(1);
  }
  logger.info('READY');
  process.exit(0);
}

main().catch((err) => {
  logger.error(err);
  process.exit(1);
});
