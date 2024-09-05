#!/usr/bin/env -S pnpm tsx
import { testFundRedeem } from '../tests/fund_redeem';

await testFundRedeem.execute();
