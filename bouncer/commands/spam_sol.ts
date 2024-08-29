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

import { amountToFineAmount, assetDecimals, executeWithTimeout, getEncodedSolAddress, getSolConnection, getSolWhaleKeyPair } from '../shared/utils';
import { ComputeBudgetProgram, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';

async function main() {
    const solanaAddress = process.argv[2];
    const solAmount = process.argv[3].trim();
    const numberOfDeposits = Number(process.argv[4].trim());
  
    console.log('Transferring ' + solAmount + ' Sol to ' + solanaAddress + ' ' + numberOfDeposits + ' times');
  
    const toPubkey = new PublicKey(getEncodedSolAddress(solanaAddress));
    const lamports = BigInt(amountToFineAmount(solAmount, assetDecimals('Sol')));
  
    const connection = getSolConnection();
    const whaleKeypair = getSolWhaleKeyPair();
  
    const txPromises = [];
    const baseTransaction = new Transaction();
    baseTransaction.add(
        ComputeBudgetProgram.setComputeUnitLimit({
        units: 600,
        })
    );
    baseTransaction.add(
        SystemProgram.transfer({
        fromPubkey: whaleKeypair.publicKey,
        toPubkey,
        lamports,
        })
    );
    
    for (let i = 0; i < numberOfDeposits; i++) {
      const transaction = baseTransaction   
 
      const txPromise = connection.sendTransaction(
        transaction,
        [whaleKeypair],
        { skipPreflight: true }
      );
      txPromises.push(txPromise);
    }
  
    const txs = await Promise.all(txPromises);
    txs.forEach((tx, index) => console.log(`${index}: ${tx}`));
  }

await executeWithTimeout(main(), 60);
