import { TestContext } from 'shared/utils/test_context';
import {
  assetContractId,
  brokerMutex,
  createStateChainKeypair,
  getEvmEndpoint,
  getEvmWhaleKeypair,
  getSolWhaleKeyPair,
  handleSubstrateError,
} from 'shared/utils';
import { u8aToHex } from '@polkadot/util';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';
import { Struct, u32, Enum, u128, u8, str } from 'scale-ts';
import { globalLogger } from 'shared/utils/logger';
import { InternalAsset } from '@chainflip/cli';

// TODO: Update these with the rpc encoding once the logic is implemented.
export const UserActionsCodec = Enum({
  Lending: Enum({
    Borrow: Struct({
      amount: u128,
      // Should be an Asset type but we simplify
      collateralAsset: u8,
      borrowAsset: u8,
    }),
  }),
});

export const TransactionMetadata = Struct({
  nonce: u32,
  expiryBlock: u32,
});
export const ChainNameCodec = str;

// Example values
const nonce = 1;
const expiryBlock = 10000;
const amount = 1234;
const collateralAsset = { asset: 'Btc' as InternalAsset, scAsset: 'Bitcoin-BTC' };
const borrowAsset = { asset: 'Usdc' as InternalAsset, scAsset: 'Ethereum-USDC' };
// For now hardcoded in the SC. It should be network dependent.
const chainName = 'Chainflip-Development';

export function encodePayloadToSign(
  payload: Uint8Array,
  userNonce: number,
  userExpiryBlock: number,
) {
  const transactionMetadata = TransactionMetadata.enc({
    nonce: userNonce,
    expiryBlock: userExpiryBlock,
  });
  const chainNameBytes = ChainNameCodec.enc(chainName);
  return new Uint8Array([...payload, ...chainNameBytes, ...transactionMetadata]);
}

export async function testOffchainSignedAction(testContext: TestContext) {
  const logger = testContext.logger;
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair('//BROKER_1');
  const whaleKeypair = getSolWhaleKeyPair();

  const action = UserActionsCodec.enc({
    tag: 'Lending',
    value: {
      tag: 'Borrow',
      value: {
        amount: BigInt(amount),
        collateralAsset: assetContractId(collateralAsset.asset),
        borrowAsset: assetContractId(borrowAsset.asset),
      },
    },
  });

  const hexAction = u8aToHex(action);
  const payload = encodePayloadToSign(action, nonce, expiryBlock);
  const hexPayload = u8aToHex(payload);

  logger.info('Signing and submitting user-signed payload with Solana wallet');

  const prefixBytes = Buffer.from([0xff, ...Buffer.from('solana offchain', 'utf8')]);
  const solPrefixedMessage = Buffer.concat([prefixBytes, payload]);
  const solHexPrefixedMessage = '0x' + solPrefixedMessage.toString('hex');
  console.log('solPrefixedMessage:', solPrefixedMessage);
  console.log('SolPrefixed Message (hex):', solHexPrefixedMessage);

  const signature = sign(solPrefixedMessage, whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + whaleKeypair.publicKey.toBuffer().toString('hex');
  console.log('Payload (hex):', hexPayload);
  console.log('Sol Signature (hex):', hexSignature);
  console.log('Signer (hex):', hexSigner);

  await brokerMutex.runExclusive(async () => {
    const brokerNonce = await chainflip.rpc.system.accountNextIndex(broker.address);
    await chainflip.tx.environment
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

  await observeEvent(globalLogger, `environment:UserSignedTransactionSubmitted`, {
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

  logger.info('Signing and submitting user-signed payload with EVM wallet using personal_sign');

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
    await chainflip.tx.environment
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

  await observeEvent(globalLogger, `environment:UserSignedTransactionSubmitted`, {
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

  logger.info('Signing and submitting user-signed payload with EVM wallet using EIP-712');

  // EIP-712 signing

  const domain = {
    name: chainName,
    version: '0',
  };

  const types = {
    Metadata: [
      { name: 'from', type: 'address' },
      { name: 'nonce', type: 'uint256' },
      { name: 'expiryBlock', type: 'uint256' },
    ],
    Borrow: [
      { name: 'amount', type: 'uint256' },
      { name: 'collateralAsset', type: 'string' },
      { name: 'borrowAsset', type: 'string' },
      { name: 'metadata', type: 'Metadata' },
    ],
  };

  const message = {
    amount,
    collateralAsset: collateralAsset.scAsset,
    borrowAsset: borrowAsset.scAsset,
    metadata: {
      from: evmSigner,
      nonce,
      expiryBlock,
    },
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
    await chainflip.tx.environment
      .submitUserSignedPayload(
        // The  EIP-712 payload will be build in the State chain previous to signature verification
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

  await observeEvent(globalLogger, `environment:UserSignedTransactionSubmitted`, {
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
