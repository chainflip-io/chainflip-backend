#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../tests/all_swaps';

await testAllSwaps.runAndExit();
