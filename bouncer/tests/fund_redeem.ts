#!/usr/bin/env -S pnpm tsx
import { testFundRedeem } from '../shared/fund_redeem';

await testFundRedeem.execute();
