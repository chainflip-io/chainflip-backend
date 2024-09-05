#!/usr/bin/env -S pnpm tsx
import { testDCASwaps } from '../shared/DCA_test';

await testDCASwaps.execute();
