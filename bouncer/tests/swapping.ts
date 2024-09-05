#!/usr/bin/env -S pnpm tsx
import { testAllSwaps } from '../shared/swapping';

await testAllSwaps.execute();
