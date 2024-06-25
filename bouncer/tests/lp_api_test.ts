#!/usr/bin/env -S pnpm tsx
import { testLpApi } from '../shared/lp_api_test';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(testLpApi(), 200);
