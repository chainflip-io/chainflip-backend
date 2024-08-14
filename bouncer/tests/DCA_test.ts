#!/usr/bin/env -S pnpm tsx
import { testDCASwaps } from '../shared/DCA_test';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(testDCASwaps(), 150);
