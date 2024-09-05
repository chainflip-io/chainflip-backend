#!/usr/bin/env -S pnpm tsx
import { testGasLimitCcmSwaps } from '../shared/gaslimit_ccm';

await testGasLimitCcmSwaps.execute();
