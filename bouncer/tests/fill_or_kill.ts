#!/usr/bin/env -S pnpm tsx
import { testFillOrKill } from '../shared/fill_or_kill';

await testFillOrKill.execute();
