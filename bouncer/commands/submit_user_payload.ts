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
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';
import { Struct, u32, Enum } from 'scale-ts';
import { globalLogger } from 'shared/utils/logger';

// TODO: Update these with the rpc encoding once the logic is implemented.
export const UserActionsCodec = Enum({
  Lending: Enum({
    Borrow: Struct({}),
  }),
});

export const ReplayProtectionCoded = Struct({
  nonce: u32,
  expiryBlock: u32,
});

export function encodePayloadToSign(
  payload: Uint8Array,
  nonce: number,
  expiryBlock: number,
  genesisHash?: Uint8Array,
) {
  const replayProtection = ReplayProtectionCoded.enc({
    nonce,
    expiryBlock,
  });
  // For now hardcoded in the SC to the Persa genesis hash
  const hash =
    genesisHash ??
    Buffer.from('7a5d4db858ada1d20ed6ded4933c33313fc9673e5fffab560d0ca714782f2080', 'hex');
  return new Uint8Array([...payload, ...hash, ...replayProtection]);
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
  const expiryBlock = 10000;
  const hexAction = u8aToHex(action);
  const payload = encodePayloadToSign(action, nonce, expiryBlock);
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

  await observeEvent(globalLogger, `swapping:UserSignedTransactionSubmitted`, {
    test: (event) => {
      const valid = event.data.valid === true && event.data.expired === false;
      const matchDecodedAction = event.data.decodedAction.Lending === 'Borrow';
      const matchSignedPayload = event.data.signedPayload === hexPayload;
      const matchUserSignatureData =
        event.data.userSignatureData?.Solana &&
        event.data.userSignatureData.Solana.signature.toLowerCase() ===
          hexSignature.toLowerCase() &&
        event.data.userSignatureData.Solana.signer.toLowerCase() === hexSigner.toLowerCase();
      return valid && matchDecodedAction && matchSignedPayload && matchUserSignatureData;
    },
    historicalCheckBlocks: 10,
  }).event;

  console.log('Submitted user signed payload in SVM');

  console.log('Trying with EVM');

  // Create the Ethereum-prefixed message
  const prefix = `\x19Ethereum Signed Message:\n${payload.length}`;
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), action]);
  const hexPrefixedMessage = '0x' + prefixedMessage.toString('hex');
  console.log('Prefixed Message (hex):', hexPrefixedMessage);

  const { privkey: whalePrivKey, pubkey: evmSigner } = getEvmWhaleKeypair('Ethereum');
  const ethWallet = new Wallet(whalePrivKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  if (evmSigner.toLowerCase() !== ethWallet.address.toLowerCase()) {
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
          expiryBlock,
        },
        {
          Ethereum: {
            signature: evmSignature,
            signer: evmSigner,
          },
        },
      )
      .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  });

  await observeEvent(globalLogger, `swapping:UserSignedTransactionSubmitted`, {
    test: (event) => {
      const valid = event.data.valid === true && event.data.expired === false;
      const matchDecodedAction = event.data.decodedAction.Lending === 'Borrow';
      const matchSignedPayload = event.data.signedPayload === hexPayload;
      const matchUserSignatureData =
        event.data.userSignatureData?.Ethereum &&
        event.data.userSignatureData.Ethereum.signature.toLowerCase() ===
          evmSignature.toLowerCase() &&
        event.data.userSignatureData.Ethereum.signer.toLowerCase() === evmSigner.toLowerCase();
      return valid && matchDecodedAction && matchSignedPayload && matchUserSignatureData;
    },
    historicalCheckBlocks: 10,
  }).event;
}

await runWithTimeoutAndExit(main(), 20);
