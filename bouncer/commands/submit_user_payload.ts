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
import { btcClient } from 'shared/send_btc';
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
  // TODO: We should also add some kind of nonce to make sure messages can't be
  // replayed. Will we need to store that in the SC in some way too.
  const messageBytes = Buffer.from(exampleMessage, 'utf8'); // Raw message bytes
  const prefix = `\x19Ethereum Signed Message:\n${messageBytes.length}`; // Prefix
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), messageBytes]); // Concatenate prefix + message

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

  // BTC not working due to pub/priv key types
  // console.log('Trying with BTC');

  // const btcHexPayload = "1122334455667788991011121314151611223344556677889910111213141516"
  // const btcSigner = await btcClient.getNewAddress('', 'legacy');
  // console.log('Using address:', btcSigner);
  // const btcSignature = await btcClient.signMessage(btcSigner, btcHexPayload);
  // console.log('Signature:', btcSignature);

  // // Decode from Base64
  // const sigBytes = Buffer.from(btcSignature, 'base64');

  // // Check length
  // if (sigBytes.length !== 65) {
  //   throw new Error(`Unexpected signature length: ${sigBytes.length}`);
  // }

  // // Drop the first byte (recovery id)
  // const sig64 = sigBytes.slice(1); // this is 64

  // // Get public key for that address
  // const info = await btcClient.getAddressInfo(btcSigner);
  // if (!info.pubkey) throw new Error('Address has no pubkey in wallet');

  // const pubkeyBytes = Buffer.from(info.pubkey, 'hex'); // 33 bytes
  // if (pubkeyBytes.length !== 33) throw new Error('Expected compressed pubkey (33 bytes)');

  // const xBytes = pubkeyBytes.slice(1); // 32 bytes
  // const yParity = pubkeyBytes[0] === 0x02 ? 0 : 1;

  // console.log('x (32 bytes):', xBytes.toString('hex'));
  // console.log('y parity:', yParity);

  // await brokerMutex.runExclusive(async () => {
  //   const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
  //   await chainflip.tx.swapping
  //     .submitUserSignedPayload('0x' + btcHexPayload, {
  //       Bitcoin: {
  //         signature: '0x' + sig64.toString('hex'),
  //         signer: '0x' + xBytes.toString('hex'), // 32-byte x-coordinate
  //       },
  //     })
  //     .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  // });
}

await runWithTimeoutAndExit(main(), 20);
