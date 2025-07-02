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
} from 'shared/utils';
import { signAndSendTxSol } from 'shared/send_sol';
import { Logger } from 'shared/utils/logger';

export async function sendSolUsdc(logger: Logger, solAddress: string, usdcAmount: string) {
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
  return signAndSendTxSol(logger, transaction);
}
