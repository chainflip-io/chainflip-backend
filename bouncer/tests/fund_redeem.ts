#!/usr/bin/env -S pnpm tsx
import { testFundRedeem } from '../shared/fund_redeem';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(testFundRedeem(), 520);
