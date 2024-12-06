#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes three arguments.
// The first argument specifies which Assethub asset to use.
// It will fund the Assethub address provided as the second argument with the asset amount
// provided in the second argument. The asset amount is interpreted in the assets denomination.
//
// For example: ./commands/send_hubasset.ts HubUsdc 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB 1.2
// will send 1.2 Usdc to account 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB on Assethub

import { sendHubAsset } from '../shared/send_hubasset';
import { HubAsset, runWithTimeoutAndExit } from '../shared/utils';

function parseHubAsset(asset: String) : HubAsset {
  switch (asset) {
    case 'HubUsdc':
      return "HubUsdc";
    case 'HubUsdt':
      return "HubUsdt";
    default:
      throw new Error()
  }
}

async function main() {
  const assethubAsset = process.argv[2];
  const assethubAddress = process.argv[3];
  const assetAmount = process.argv[4].trim();

  await sendHubAsset(parseHubAsset(assethubAsset), assethubAddress, assetAmount);
}

await runWithTimeoutAndExit(main(), 20);
