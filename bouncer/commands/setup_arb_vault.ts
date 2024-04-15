#!/usr/bin/env -S pnpm tsx

import { setupArbVault } from '../shared/setup_arb_vault';

setupArbVault().catch((error) => {
  console.error(error);
  process.exit(-1);
});
