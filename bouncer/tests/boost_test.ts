#!/usr/bin/env -S pnpm tsx
import { testBoostingSwap } from '../shared/boost';

await testBoostingSwap.execute();
