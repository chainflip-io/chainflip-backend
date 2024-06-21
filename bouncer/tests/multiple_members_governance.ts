#!/usr/bin/env -S pnpm tsx

import { executeWithTimeout } from '../shared/utils';
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';

await executeWithTimeout(testMultipleMembersGovernance(), 120);
