#!/usr/bin/env -S pnpm tsx
import { testDCASwaps } from '../shared/streaming_swaps';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(testDCASwaps(), 150);
