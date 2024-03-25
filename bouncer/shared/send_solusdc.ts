import { Transaction, PublicKey } from '@solana/web3.js';
import {
  createAssociatedTokenAccountIdempotentInstruction,
  createTransferInstruction,
  getAssociatedTokenAddressSync,
} from '@solana/spl-token';
import {
  amountToFineAmount,
  assetDecimals,
  getContractAddress,
  getEncodedSolAddress,
  getSolWhaleKeyPair,
} from './utils';
import { signAndSendTxSol } from './send_sol';

export async function sendSolUsdc(solAddress: string, usdcAmount: string, log = true) {
  const usdcMintPubKey = new PublicKey(getContractAddress('Solana', 'SolUsdc'));

  const whaleKeypair = getSolWhaleKeyPair();
  const whaleAta = getAssociatedTokenAddressSync(usdcMintPubKey, whaleKeypair.publicKey, false);
  const encodedSolAddress = new PublicKey(getEncodedSolAddress(solAddress));
  const receiverAta = getAssociatedTokenAddressSync(usdcMintPubKey, encodedSolAddress, true);

  const usdcFineAmount = amountToFineAmount(usdcAmount, assetDecimals('SolUsdc'));

  const transaction = new Transaction().add(
    createAssociatedTokenAccountIdempotentInstruction(
      whaleKeypair.publicKey,
      receiverAta,
      encodedSolAddress,
      usdcMintPubKey,
    ),
    createTransferInstruction(
      whaleAta,
      receiverAta,
      whaleKeypair.publicKey,
      BigInt(usdcFineAmount),
    ),
  );
  await signAndSendTxSol(transaction, log);
}
