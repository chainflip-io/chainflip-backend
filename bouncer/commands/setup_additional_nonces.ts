#!/usr/bin/env -S pnpm tsx

import { NonceAccount, PublicKey, SystemProgram, Transaction } from '@solana/web3.js';
import {
  getSolConnection,
  getSolWhaleKeyPair,
  runWithTimeoutAndExit,
  solanaNumberOfAdditionalNonces,
} from 'shared/utils';
import { signAndSendTxSol } from 'shared/send_sol';
import { globalLogger } from 'shared/utils/logger';

// This is to be used for the upgrade test to setup the additional nonces addded in the
// 1.9 release.
async function main() {
  const whaleKeypair = getSolWhaleKeyPair();

  // The simplest way to get the current aggKey is to just check the authority of a base
  // nonce since those ones have been already set up in the pre-upgrade initialization
  const baseNonceAccount = new PublicKey('2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw');
  const nonceAccountInfo = await getSolConnection().getAccountInfo(baseNonceAccount);
  const currentAggKey = NonceAccount.fromAccountData(nonceAccountInfo!.data).authorizedPubkey;

  for (const [nonceNumber, prefix] of [[solanaNumberOfAdditionalNonces, '-add-nonce']]) {
    for (let i = 0; i < Number(nonceNumber); i++) {
      const seed = prefix + i.toString();
      const nonceAccountPubKey = await PublicKey.createWithSeed(
        whaleKeypair.publicKey,
        seed,
        SystemProgram.programId,
      );

      const tx = new Transaction().add(
        SystemProgram.nonceAuthorize({
          noncePubkey: new PublicKey(nonceAccountPubKey),
          authorizedPubkey: whaleKeypair.publicKey,
          newAuthorizedPubkey: currentAggKey,
        }),
      );
      await signAndSendTxSol(globalLogger, tx);
    }
  }
}

await runWithTimeoutAndExit(main(), 120);
