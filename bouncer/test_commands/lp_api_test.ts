#!/usr/bin/env -S pnpm tsx
import { testLpApi } from '../tests/lp_api_test';

await testLpApi.runAndExit();
