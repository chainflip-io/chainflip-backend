#!/usr/bin/env -S pnpm tsx

import { runWithTimeout } from '../shared/utils';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';

runWithTimeout(testMultipleMembersGovernance(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
