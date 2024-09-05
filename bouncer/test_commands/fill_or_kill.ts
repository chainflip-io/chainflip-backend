#!/usr/bin/env -S pnpm tsx
import { testFillOrKill } from '../tests/fill_or_kill';

await testFillOrKill.execute();
