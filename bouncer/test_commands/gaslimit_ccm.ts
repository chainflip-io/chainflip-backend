#!/usr/bin/env -S pnpm tsx
import { testGasLimitCcmSwaps } from '../tests/gaslimit_ccm';

await testGasLimitCcmSwaps.runAndExit();
