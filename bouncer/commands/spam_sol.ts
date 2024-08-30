#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes three arguments.
// It will fund the solana address provided as the first argument with the amount
// provided in the second argument. The third is the number of successive deposits.
// The asset amount is interpreted in Sol
//
// For example: ./commands/spam_sol.ts 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE 0.01 100
// will send 0.01 * 100 Sol to account 7QQGNm3ptwinipDCyaCF7jY5katgmFUu1ieP2f7nwLpE
// It also accepts non-encoded bs58 address representations:
// ./commands/spam_sol.ts 0x2f3fcadf740018f6037513959bab60d0dbef26888d264d54fc4d3d36c8cf5c91 0.01 100

import BigNumber from 'bignumber.js';
import { sendSol } from '../shared/send_sol';
import { executeWithTimeout } from '../shared/utils';

async function main() {
  const solanaAddress = process.argv[2];
  let solAmount = new BigNumber(process.argv[3].trim());
  const numberOfDeposits = Number(process.argv[4].trim());

  console.log(
    'Transferring ' + solAmount + ' Sol to ' + solanaAddress + ' ' + numberOfDeposits + ' times',
  );

  const txPromises = [];

  for (let i = 0; i < numberOfDeposits; i++) {
    // Add a minimal amount so they are not the same transaction and end up silently failing
    // in the background given that the underneath client might use the same PoH hash.
    solAmount = solAmount.plus(new BigNumber('0.000000001'));
    txPromises.push(sendSol(solanaAddress, solAmount.toString(), false));
  }

  const txs = await Promise.all(txPromises);
  txs.forEach((tx) => console.log('tx: ', tx?.transaction?.signatures[0]));
}

await executeWithTimeout(main(), 10);
