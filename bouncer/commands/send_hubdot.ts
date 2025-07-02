#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the Assethub address provided as the first argument with the DOT amount
// provided in the second argument. The asset amount is interpreted in Dot.
//
// For example: ./commands/send_hubdot.ts 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB 1.2
// will send 1.2 Dot to account 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB on Assethub

import { sendHubAsset } from 'shared/send_hubasset';
import { runWithTimeoutAndExit } from 'shared/utils';
import { globalLogger } from 'shared/utils/logger';

async function main() {
  const polkadotAddress = process.argv[2];
  const dotAmount = process.argv[3].trim();

  await sendHubAsset(globalLogger, 'HubDot', polkadotAddress, dotAmount);
}

await runWithTimeoutAndExit(main(), 20);
