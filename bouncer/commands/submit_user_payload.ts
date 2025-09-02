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
import { Struct, u32, Enum, u128 } from 'scale-ts';
import { globalLogger } from 'shared/utils/logger';

// TODO: Update these with the rpc encoding once the logic is implemented.
export const UserActionsCodec = Enum({
  Lending: Enum({
    Borrow: Struct({
      amount: u128,
      collateralAsset: u128,
      borrowAsset: u128,
    }),
  }),
});

export const UserMetadataCodec = Struct({
  nonce: u32,
  expiryBlock: u32,
});

// For now hardcoded in the SC. It should be network dependent.
const chainId = 1;

export function encodePayloadToSign(payload: Uint8Array, nonce: number, expiryBlock: number) {
  const userMetadata = UserMetadataCodec.enc({
    nonce,
    expiryBlock,
  });
  return new Uint8Array([...payload, ...new Uint8Array([chainId]), ...userMetadata]);
}
async function main() {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair('//BROKER_1');
  const whaleKeypair = getSolWhaleKeyPair();

  // Example values
  const nonce = 1;
  const expiryBlock = 10000;
  const amount = 1234;
  const collateralAsset = 5;
  const borrowAsset = 3;

  const action = UserActionsCodec.enc({
    tag: 'Lending',
    value: {
      tag: 'Borrow',
      value: {
        amount: BigInt(amount),
        collateralAsset: BigInt(collateralAsset),
        borrowAsset: BigInt(borrowAsset),
      },
    },
  });

  const hexAction = u8aToHex(action);
  const payload = encodePayloadToSign(action, nonce, expiryBlock);
  const hexPayload = u8aToHex(payload);

  const prefixBytes = Buffer.from([0xff, ...Buffer.from('solana offchain', 'utf8')]);
  const solPrefixedMessage = Buffer.concat([prefixBytes, payload]);
  const solHexPrefixedMessage = '0x' + solPrefixedMessage.toString('hex');
  console.log('SolPrefixed Message (hex):', solHexPrefixedMessage);

  const signature = sign(solPrefixedMessage, whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + whaleKeypair.publicKey.toBuffer().toString('hex');
  console.log('Payload (hex):', hexPayload);
  console.log('Signed Message (hex):', hexSignature);
  console.log('Signer (hex):', hexSigner);

  await brokerMutex.runExclusive(async () => {
    const brokerNonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.swapping
      .submitUserSignedPayload(
        // Solana prefix will be added in the SC previous to signature verification
        hexAction,
        {
          nonce,
          expiryBlock,
        },
        {
          Solana: {
            signature: hexSignature,
            signer: hexSigner,
            sigType: 'Domain',
          },
        },
      )
      .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  });

  await observeEvent(globalLogger, `swapping:UserSignedTransactionSubmitted`, {
    test: (event) => {
      const valid = event.data.valid === true && event.data.expired === false;
      const matchDecodedAction = !!event.data.decodedAction.Lending?.Borrow;
      const matchSignedPayload = event.data.signedPayload === solHexPrefixedMessage;
      const matchUserSignatureData =
        event.data.userSignatureData?.Solana &&
        event.data.userSignatureData.Solana.signature.toLowerCase() ===
          hexSignature.toLowerCase() &&
        event.data.userSignatureData.Solana.signer.toLowerCase() === hexSigner.toLowerCase();
      return valid && matchDecodedAction && matchSignedPayload && matchUserSignatureData;
    },
    historicalCheckBlocks: 1,
  }).event;

  console.log('Submitted user signed payload in SVM');

  console.log('Trying with EVM');

  // Create the Ethereum-prefixed message
  const prefix = `\x19Ethereum Signed Message:\n${payload.length}`;
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), payload]);
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
            sig_type: 'Domain',
          },
        },
      )
      .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  });

  await observeEvent(globalLogger, `swapping:UserSignedTransactionSubmitted`, {
    test: (event) => {
      const valid = event.data.valid === true && event.data.expired === false;
      const matchDecodedAction = !!event.data.decodedAction.Lending?.Borrow;
      const matchSignedPayload = event.data.signedPayload === hexPrefixedMessage;
      const matchUserSignatureData =
        event.data.userSignatureData?.Ethereum &&
        event.data.userSignatureData.Ethereum.signature.toLowerCase() ===
          evmSignature.toLowerCase() &&
        event.data.userSignatureData.Ethereum.signer.toLowerCase() === evmSigner.toLowerCase();
      return valid && matchDecodedAction && matchSignedPayload && matchUserSignatureData;
    },
    historicalCheckBlocks: 1,
  }).event;

  console.log('Trying EVM EIP712');

  const domain = {
    name: 'Chainflip',
    version: '0',
    chainId: 1,
  };

  const types = {
    Borrow: [
      { name: 'from', type: 'string' },
      { name: 'amount', type: 'uint256' },
      { name: 'collateralAsset', type: 'uint256' },
      { name: 'borrowAsset', type: 'uint256' },
    ],
  };

  // The data to sign
  const message = {
    from: evmSigner,
    amount,
    collateralAsset,
    borrowAsset,
  };

  const evmSignatureEip712 = await ethWallet.signTypedData(domain, types, message);
  console.log('EIP712 Signature:', evmSignatureEip712);

  const encodedPayload = ethers.TypedDataEncoder.encode(domain, types, message);
  console.log('EIP-712 Encoded Payload:', encodedPayload);
  const hash = ethers.TypedDataEncoder.hash(domain, types, message);
  console.log('EIP-712 Hash:', hash);
  const hashDomain = ethers.TypedDataEncoder.hashDomain(domain);
  console.log('EIP-712 Domain Hash:', hashDomain);
  const messageHash = ethers.TypedDataEncoder.from(types).hash(message);
  console.log('EIP-712 Message Hash:', messageHash);

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
            signature: evmSignatureEip712,
            signer: evmSigner,
            sig_type: 'Eip712',
          },
        },
      )
      .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  });

  await observeEvent(globalLogger, `swapping:UserSignedTransactionSubmitted`, {
    test: (event) => {
      const valid = event.data.valid === true && event.data.expired === false;
      const matchDecodedAction = !!event.data.decodedAction.Lending?.Borrow;
      const matchSignedPayload = event.data.signedPayload === encodedPayload;
      const matchUserSignatureData =
        event.data.userSignatureData?.Ethereum &&
        event.data.userSignatureData.Ethereum.signature.toLowerCase() ===
          evmSignatureEip712.toLowerCase() &&
        event.data.userSignatureData.Ethereum.signer.toLowerCase() === evmSigner.toLowerCase();
      return valid && matchDecodedAction && matchSignedPayload && matchUserSignatureData;
    },
    historicalCheckBlocks: 1,
  }).event;
}

await runWithTimeoutAndExit(main(), 20);
