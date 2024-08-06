#!/usr/bin/env -S pnpm tsx
import { testDCASwap } from '../shared/streaming_swaps';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(testDCASwap(), 120);
