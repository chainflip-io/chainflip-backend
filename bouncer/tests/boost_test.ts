#!/usr/bin/env -S pnpm tsx
import { executeWithTimeout } from '../shared/utils';
import { testBoostingSwap } from '../shared/boost';

await executeWithTimeout(testBoostingSwap(), 120);
