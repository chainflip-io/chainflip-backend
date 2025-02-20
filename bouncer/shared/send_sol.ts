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
import { Logger } from './utils/logger';

export async function signAndSendTxSol(logger: Logger, transaction: Transaction) {
  const connection = getSolConnection();
  const whaleKeypair = getSolWhaleKeyPair();
  const tx = transaction;

  tx.recentBlockhash = (await connection.getLatestBlockhash()).blockhash;
  tx.sign(whaleKeypair);
  const txHash = await connection.sendRawTransaction(tx.serialize());
  await connection.confirmTransaction(txHash);

  const receipt = await connection.getParsedTransaction(txHash);

  logger.debug(`Transaction complete, tx_hash: ${txHash} at slot: ${receipt!.slot}`);

  return receipt;
}

export async function signAndSendIxsSol(
  logger: Logger,
  instructions: TransactionInstruction[],
  prioFee = 0,
  limitCU = 0,
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

  return signAndSendTxSol(logger, transaction);
}

export async function sendSol(logger: Logger, solAddress: string, solAmount: string) {
  const lamportsAmount = amountToFineAmount(solAmount, assetDecimals('Sol'));

  const transaction = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: getSolWhaleKeyPair().publicKey,
      toPubkey: new PublicKey(getEncodedSolAddress(solAddress)),
      lamports: BigInt(lamportsAmount),
    }),
  );
  return signAndSendTxSol(logger, transaction);
}
