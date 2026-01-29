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

export async function sendSolUsdt(logger: Logger, solAddress: string, usdtAmount: string) {
  const usdtMintPubKey = new PublicKey(getContractAddress('Solana', 'SolUsdt'));

  const whaleKeypair = getSolWhaleKeyPair();
  const whaleAta = getAssociatedTokenAddressSync(usdtMintPubKey, whaleKeypair.publicKey, false);
  const encodedSolAddress = new PublicKey(getEncodedSolAddress(solAddress));
  const receiverAta = getAssociatedTokenAddressSync(usdtMintPubKey, encodedSolAddress, true);

  const usdtFineAmount = amountToFineAmount(usdtAmount, assetDecimals('SolUsdt'));

  const transaction = new Transaction().add(
    createAssociatedTokenAccountIdempotentInstruction(
      whaleKeypair.publicKey,
      receiverAta,
      encodedSolAddress,
      usdtMintPubKey,
    ),
    createTransferInstruction(
      whaleAta,
      receiverAta,
      whaleKeypair.publicKey,
      BigInt(usdtFineAmount),
    ),
  );
  return signAndSendTxSol(logger, transaction);
}
