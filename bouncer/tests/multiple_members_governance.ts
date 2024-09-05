#!/usr/bin/env -S pnpm tsx
import { testMultipleMembersGovernance } from '../shared/multiple_members_governance';

await testMultipleMembersGovernance.execute();
