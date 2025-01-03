#!/usr/bin/env -S pnpm tsx
import { legacyEvmVaultSwaps } from '../tests/legacy_vault_swap';

await legacyEvmVaultSwaps.runAndExit();
