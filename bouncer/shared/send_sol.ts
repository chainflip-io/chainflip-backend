import { Transaction, SystemProgram, PublicKey } from '@solana/web3.js';
import { amountToFineAmount, getSolConnection, getSolWhaleKeyPair } from './utils';

export async function signAndSendTxSol(tx: Transaction, /* gas = 2000000, */ log = true) {
  const connection = getSolConnection();
  const whaleKeypair = getSolWhaleKeyPair();

  const transaction = tx;
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
  const transaction = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: getSolWhaleKeyPair().publicKey,
      toPubkey: new PublicKey(solAddress),
      lamports: BigInt(lamportsAmount),
    }),
  );
  await signAndSendTxSol(transaction, /* undefined, */ log);
}
