#!/usr/bin/env -S pnpm tsx
import { testDCASwaps } from '../tests/DCA_test';

await testDCASwaps.runAndExit();
