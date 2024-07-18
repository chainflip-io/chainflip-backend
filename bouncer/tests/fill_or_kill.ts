#!/usr/bin/env -S pnpm tsx
import { executeWithTimeout } from '../shared/utils';
import { testFillOrKill } from '../shared/fill_or_kill';

await executeWithTimeout(testFillOrKill(), 160);
