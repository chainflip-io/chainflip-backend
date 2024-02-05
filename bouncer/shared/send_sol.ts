import {
  Transaction,
  SystemProgram,
  PublicKey,
  TransactionInstruction,
  ComputeBudgetProgram,
} from '@solana/web3.js';
import { amountToFineAmount, getSolConnection, getSolWhaleKeyPair } from './utils';

export async function signAndSendTxSol(
  instructions: TransactionInstruction[],
  prioFee = 0,
  limitCU = 0,
  log = true,
) {
  const connection = getSolConnection();
  const whaleKeypair = getSolWhaleKeyPair();

  const transaction = new Transaction();

  if (prioFee > 0) {
    transaction.add(
      ComputeBudgetProgram.setComputeUnitPrice({
        microLamports: prioFee,
      }),
    );
  }

  if (limitCU > 0) {
    transaction.add(
      ComputeBudgetProgram.setComputeUnitLimit({
        units: limitCU,
      }),
    );
  }

  // Add remaining instructions
  instructions.forEach((item) => {
    transaction.add(item);
  });

  transaction.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  transaction.sign(whaleKeypair);
  const txHash = await connection.sendRawTransaction(transaction.serialize());
  await connection.confirmTransaction(txHash);

  const receipt = await connection.getParsedTransaction(txHash);

  if (log) {
    console.log('Transaction complete, tx_hash: ' + txHash + ' at slot: ' + receipt!.slot);
  }
  return receipt;
}

export async function sendSol(solAddress: string, solAmount: string, log = true) {
  const lamportsAmount = amountToFineAmount(solAmount, 9); // assetDecimals.SOL when available
  const intruction = [
    SystemProgram.transfer({
      fromPubkey: getSolWhaleKeyPair().publicKey,
      toPubkey: new PublicKey(solAddress),
      lamports: BigInt(lamportsAmount),
    }),
  ];
  await signAndSendTxSol(intruction, undefined, undefined, log);
}
