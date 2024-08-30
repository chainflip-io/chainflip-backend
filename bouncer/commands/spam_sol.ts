#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// It will repeatedly send Sol or SolUsdc to the provided address concurrently and as
// fast as possible, with many transactions potentially being included in the same slot.
// <asset> <destAsset> <amount> <numberOfTransfers>
// The asset amount is interpreted in Sol/SolUsdc.
//
// For example: ./commands/spam_sol.ts Sol 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 0.01 100
// will send 0.01 * 100 Sol to account 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE
// It also accepts non-encoded bs58 address representations:
// ./commands/spam_sol.ts Sol 0x2f3fcadf740018f6037513959bab60d0dbef26888d264d54fc4d3d36c8cf5c91 0.01 100

import BigNumber from 'bignumber.js';
import { sendSol } from '../shared/send_sol';
import { assetDecimals, executeWithTimeout } from '../shared/utils';
import { sendSolUsdc } from '../shared/send_solusdc';

async function main() {
  const asset = process.argv[2].trim() as 'Sol' | 'SolUsdc';
  const solanaAddress = process.argv[3];
  let solAmount = new BigNumber(process.argv[4].trim());
  const numberOfDeposits = Number(process.argv[5].trim());

  console.log(
    'Transferring ' +
      solAmount +
      ' ' +
      asset +
      ' to ' +
      solanaAddress +
      ' ' +
      numberOfDeposits +
      ' times',
  );

  const txPromises = [];
  const decimals = assetDecimals(asset);

  for (let i = 0; i < numberOfDeposits; i++) {
    // Add a minimal amount so they are not the same transaction and end up silently failing
    // in the background given that the underneath client might use the same PoH hash.
    solAmount = solAmount.plus(new BigNumber(1).div(10 ** decimals));
    switch (asset) {
      case 'Sol':
        txPromises.push(sendSol(solanaAddress, solAmount.toString(), false));
        break;
      case 'SolUsdc':
        txPromises.push(sendSolUsdc(solanaAddress, solAmount.toString(), false));
        break;
      default:
        throw new Error('Unsupported asset');
    }
  }

  const txs = await Promise.all(txPromises);
  txs.forEach((tx) => console.log('tx: ', tx?.transaction?.signatures[0]));
}

await executeWithTimeout(main(), 10);
