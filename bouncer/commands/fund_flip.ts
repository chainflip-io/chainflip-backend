#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the public key provided as the first argument with the amount
// provided in the second argument. The asset amount is interpreted as FLIP
//
// For example: ./commands/fund_flip.ts 0x5f2b0c89b9f7f240c2aab5cc3118f51f8ba7d4dfb9cd2a1abd6ea4d327bcd34c 5.5
// will fund 5.5 FLIP to the account with public key 0x5f2b0c89b9f7f240c2aab5cc3118f51f8ba7d4dfb9cd2a1abd6ea4d327bcd34c
// (That would be account cFL2GAaTbP6UHgfQwJuJ7Naq6gh7ZxZiWQ8EcmdYeopGhpziQ)

import { runWithTimeoutAndExit } from '../shared/utils';
import { fundFlip } from '../shared/fund_flip';
import { globalLogger } from '../shared/utils/logger';

const address = process.argv[2];
const flipAmount = process.argv[3].trim();
await runWithTimeoutAndExit(fundFlip(globalLogger, address, flipAmount), 120);
