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
import { Struct, u32, Enum } from 'scale-ts';

// TODO: Update these with the rpc encoding once the logic is implemented.
export const UserActionsCodec = Enum({
  Lending: Enum({
    Borrow: Struct({}), // Nested Borrow variant
  }),
});

export const ReplayProtectionCoded = Struct({
  nonce: u32,
  chainId: u32,
  expiryBlock: u32,
});

export function encodePayloadToSign(
  payload: Uint8Array,
  nonce: number,
  chainId: number,
  expiryBlock: number,
) {
  const replayProtection = ReplayProtectionCoded.enc({
    nonce,
    chainId,
    expiryBlock,
  });
  return new Uint8Array([...payload, ...replayProtection]);
}
async function main() {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair('//BROKER_1');
  const whaleKeypair = getSolWhaleKeyPair();

  const action = UserActionsCodec.enc({
    tag: 'Lending',
    value: { tag: 'Borrow', value: {} },
  });
  // Example values
  const nonce = 1;
  const chainId = 2;
  const expiryBlock = 10000;
  const hexAction = u8aToHex(action);
  const payload = encodePayloadToSign(action, nonce, chainId, expiryBlock);
  const hexPayload = u8aToHex(payload);

  const signature = sign(payload, whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + whaleKeypair.publicKey.toBuffer().toString('hex');
  console.log('Payload:', payload);
  console.log('Payload (hex):', hexPayload);
  console.log('Signed Message (hex):', hexSignature);
  console.log('Signer (hex):', hexSigner);

  await brokerMutex.runExclusive(async () => {
    const brokerNonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload(
        hexAction,
        {
          nonce,
          chainId,
          expiryBlock,
        },
        {
          Solana: {
            signature: hexSignature,
            signer: hexSigner,
          },
        },
      )
      .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  });

  console.log('Submitted user signed payload in SVM');

  // TODO: Add event check

  console.log('Trying with EVM');

  // Create the Ethereum-prefixed message
  const prefix = `\x19Ethereum Signed Message:\n${payload.length}`;
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), action]);
  const hexPrefixedMessage = '0x' + prefixedMessage.toString('hex');
  console.log('Prefixed Message (hex):', hexPrefixedMessage);

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
    const brokerNonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload(
        // Ethereum prefix will be added in the SC previous to signature verification
        hexAction,
        {
          nonce,
          chainId,
          expiryBlock,
        },
        {
          Ethereum: {
            signature: evmSignature,
            signer: pubkey,
          },
        },
      )
      .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  });

  // TODO: Add event check
}

await runWithTimeoutAndExit(main(), 20);
