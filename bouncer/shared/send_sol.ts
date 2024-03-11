import {
  Transaction,
  SystemProgram,
  PublicKey,
  TransactionInstruction,
  ComputeBudgetProgram,
} from '@solana/web3.js';
import {
  amountToFineAmount,
  assetDecimals,
  getEncodedSolAddress,
  getSolConnection,
  getSolWhaleKeyPair,
} from './utils';

export async function signAndSendTxSol(transaction: Transaction, log = true) {
  const connection = getSolConnection();
  const whaleKeypair = getSolWhaleKeyPair();
  const tx = transaction;

  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  tx.sign(whaleKeypair);
  const txHash = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction(txHash);

  const receipt = await connection.getParsedTransaction(txHash);

  if (log) {
    console.log('Transaction complete, tx_hash: ' + txHash + ' at slot: ' + receipt!.slot);
  }
  return receipt;
}

export async function signAndSendIxsSol(
  instructions: TransactionInstruction[],
  prioFee = 0,
  limitCU = 0,
  log = true,
) {
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

  // Add instructions
  instructions.forEach((item) => {
    transaction.add(item);
  });

  return signAndSendTxSol(transaction, log);
}

export async function sendSol(solAddress: string, solAmount: string, log = true) {
  const lamportsAmount = amountToFineAmount(solAmount, assetDecimals("SOL"));

  const transaction = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: getSolWhaleKeyPair().publicKey,
      toPubkey: new PublicKey(getEncodedSolAddress(solAddress)),
      lamports: BigInt(lamportsAmount),
    }),
  );
  await signAndSendTxSol(transaction, log);
}
