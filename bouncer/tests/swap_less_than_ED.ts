#!/usr/bin/env -S pnpm tsx
import { swapLessThanED } from '../shared/swap_less_than_existential_deposit_dot';
import { executeWithTimeout } from '../shared/utils';

await executeWithTimeout(swapLessThanED(), 300);
