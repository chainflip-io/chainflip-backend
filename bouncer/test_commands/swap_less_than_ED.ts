#!/usr/bin/env -S pnpm tsx
import { swapLessThanED } from '../tests/swap_less_than_existential_deposit_dot';

await swapLessThanED.runAndExit();
