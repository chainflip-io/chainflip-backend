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
import { u8aToHex } from '@polkadot/util';
import { getChainflipApi } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';
import { Struct, u32 } from 'scale-ts';

// TODO: Update this with the rpc encoding once the logic is implemented.
export const ReplayProtectionCoded = Struct({
  nonce: u32,
  chainId: u32,
});

export function encodePayloadToSign(payload: Uint8Array) {
  const replayProtection = ReplayProtectionCoded.enc({
    nonce: 0,
    chainId: 1,
  });
  // Concatenate payload
  return new Uint8Array([...payload, ...replayProtection]);
}
async function main() {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair('//BROKER_1');
  const whaleKeypair = getSolWhaleKeyPair();

  const arbitraryPayload = '0x1234';
  const payload = encodePayloadToSign(Buffer.from(arbitraryPayload.slice(2), 'hex'));
  const hexPayload = u8aToHex(payload);

  const signature = sign(payload, whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + whaleKeypair.publicKey.toBuffer().toString('hex');
  console.log('Payload:', payload);
  console.log('Payload (hex):', hexPayload);
  console.log('Signed Message (hex):', hexSignature);
  console.log('Signer (hex):', hexSigner);

  await brokerMutex.runExclusive(async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload(
        arbitraryPayload,
        {
          nonce: 0,
          chainId: 1,
        },
        {
          Solana: {
            signature: hexSignature,
            signer: hexSigner,
          },
        },
      )
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });

  console.log('Submitted user signed payload in SVM');

  // TODO: Add event check

  console.log('Trying with EVM');

  // Create the Ethereum-prefixed message
  const prefix = `\x19Ethereum Signed Message:\n${payload.length}`;
  const prefixedMessage = Buffer.concat([
    Buffer.from(prefix, 'utf8'),
    Buffer.from(arbitraryPayload.slice(2), 'hex'),
  ]);
  const hexPrefixedMessage = '0x' + prefixedMessage.toString('hex');

  const { privkey: whalePrivKey, pubkey } = getEvmWhaleKeypair('Ethereum');
  const ethWallet = new Wallet(whalePrivKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  if (pubkey.toLowerCase() !== ethWallet.address.toLowerCase()) {
    throw new Error('Address does not match expected pubkey');
  }

  const evmSignature = await ethWallet.signMessage(payload);
  console.log('evmSignature:', evmSignature);
  console.log('compressed pubkey', ethWallet.signingKey.compressedPublicKey);

  await brokerMutex.runExclusive(async () => {
    const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload(
        hexPrefixedMessage,
        {
          nonce: 0,
          chainId: 1,
        },
        {
          Ethereum: {
            signature: evmSignature,
            signer: pubkey,
          },
        },
      )
      .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));
  });

  // TODO: Add event check
}

await runWithTimeoutAndExit(main(), 20);
