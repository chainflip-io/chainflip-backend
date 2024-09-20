#!/usr/bin/env -S pnpm tsx
import { testBoostingSwap } from '../tests/boost';

await testBoostingSwap.runAndExit();
