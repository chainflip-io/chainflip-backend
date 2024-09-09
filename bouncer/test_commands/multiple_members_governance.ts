#!/usr/bin/env -S pnpm tsx
import { testMultipleMembersGovernance } from '../tests/multiple_members_governance';

await testMultipleMembersGovernance.runAndExit();
