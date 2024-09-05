#!/usr/bin/env -S pnpm tsx
import { testLpApi } from '../shared/lp_api_test';

await testLpApi.execute();
