#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//

import {
  brokerMutex,
  createStateChainKeypair,
  getEvmEndpoint,
  getEvmWhaleKeypair,
  getSolWhaleKeyPair,
  handleSubstrateError,
  runWithTimeoutAndExit,
} from 'shared/utils';
import { getChainflipApi } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';

async function main() {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair('//BROKER_1');
  const whaleKeypair = getSolWhaleKeyPair();

  const payload = Buffer.from('Hello! This is an arbitrary message to sign.');
  const hexPayload = Buffer.from(payload).toString('hex');
  const signature = sign(payload, whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = Buffer.from(signature).toString('hex');
  console.log('Payload (hex):', hexPayload);
  console.log('Signed Message (hex):', hexSignature);
  const hexSigner = whaleKeypair.publicKey.toBuffer().toString('hex');
  console.log('Signer (hex):', hexSigner);

  await brokerMutex.runExclusive(async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload('0x' + hexPayload, {
        Solana: {
          signature: '0x' + hexSignature,
          signer: '0x' + hexSigner,
        },
      })
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });

  console.log('Submitted user signed payload in SVM');

  console.log('Trying with EVM');

  // Original message
  const exampleMessage = 'Example `personal_sign` message.';

  // Create the Ethereum-prefixed message
  const messageBytes = Buffer.from(exampleMessage, 'utf8'); // Raw message bytes
  const prefix = `\x19Ethereum Signed Message:\n${messageBytes.length}`;
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), messageBytes]);

  const { privkey: whalePrivKey, pubkey } = getEvmWhaleKeypair('Ethereum');
  const ethWallet = new Wallet(whalePrivKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  if (pubkey.toLowerCase() !== ethWallet.address.toLowerCase()) {
    throw new Error('Address does not match expected pubkey');
  }

  const evmSignature = await ethWallet.signMessage(messageBytes);
  console.log('evmSignature:', evmSignature);
  console.log('prefixedMessage (hex):', prefixedMessage.toString('hex'));
  console.log('compressed pubkey', ethWallet.signingKey.compressedPublicKey);

  await brokerMutex.runExclusive(async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload('0x' + prefixedMessage.toString('hex'), {
        Ethereum: {
          signature: evmSignature,
          signer: pubkey,
        },
      })
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });
}

await runWithTimeoutAndExit(main(), 20);
