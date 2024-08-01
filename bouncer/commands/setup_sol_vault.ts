#!/usr/bin/env -S pnpm tsx

import { setupSolVault } from '../shared/setup_sol_vault';

setupSolVault().catch((error) => {
  console.error(error);
  process.exit(-1);
});
