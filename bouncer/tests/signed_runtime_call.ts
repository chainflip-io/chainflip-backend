import { TestContext } from 'shared/utils/test_context';
import { getEvmEndpoint, getEvmWhaleKeypair, getSolWhaleKeyPair } from 'shared/utils';
import { u8aToHex } from '@polkadot/util';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';
import { Struct, u32, str /* Enum, u128, u8 */ } from 'scale-ts';
import { globalLogger } from 'shared/utils/logger';

export const TransactionMetadata = Struct({
  nonce: u32,
  expiryBlock: u32,
});
export const ChainNameCodec = str;
export const VersionCodec = str;

// Example values
const expiryBlock = 10000;
// const amount = 1234;
// const collateralAsset = { asset: 'Btc' as InternalAsset, scAsset: 'Bitcoin-BTC' };
// const borrowAsset = { asset: 'Usdc' as InternalAsset, scAsset: 'Ethereum-USDC' };
// For now hardcoded in the SC. It should be network dependent.
const chainName = 'Chainflip-Development';
const version = '0';

export function encodeDomainDataToSign(
  payload: Uint8Array,
  nonce: number,
  userExpiryBlock: number,
) {
  const transactionMetadata = TransactionMetadata.enc({
    nonce,
    expiryBlock: userExpiryBlock,
  });
  const chainNameBytes = ChainNameCodec.enc(chainName);
  const versionBytes = VersionCodec.enc(version);
  return new Uint8Array([...payload, ...chainNameBytes, ...versionBytes, ...transactionMetadata]);
}

export async function testSignedRuntimeCall(testContext: TestContext) {
  const logger = testContext.logger;
  await using chainflip = await getChainflipApi();

  const whaleKeypair = getSolWhaleKeyPair();

  // Create a simple RuntimeCall - system.remark with empty data
  const call = chainflip.tx.system.remark([]);

  // SCALE encode the RuntimeCall and convert to hex
  const runtimeCall = call.method.toU8a();
  const hexRuntimeCall = u8aToHex(runtimeCall);
  console.log('hexRuntimeCall', hexRuntimeCall);

  // SVM Whale -> SC account (`cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1`)
  const svmNonce = (await chainflip.rpc.system.accountNextIndex(
    'cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1',
  )) as unknown as number;
  const svmPayload = encodeDomainDataToSign(runtimeCall, svmNonce, expiryBlock);
  const svmHexPayload = u8aToHex(svmPayload);

  logger.info('Signing and submitting user-signed payload with Solana wallet');

  const prefixBytes = Buffer.from([0xff, ...Buffer.from('solana offchain', 'utf8')]);
  const solPrefixedMessage = Buffer.concat([prefixBytes, svmPayload]);
  const solHexPrefixedMessage = '0x' + solPrefixedMessage.toString('hex');
  console.log('solPrefixedMessage:', solPrefixedMessage);
  console.log('SolPrefixed Message (hex):', solHexPrefixedMessage);

  const signature = sign(solPrefixedMessage, whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + whaleKeypair.publicKey.toBuffer().toString('hex');
  console.log('Payload (hex):', svmHexPayload);
  console.log('Sol Signature (hex):', hexSignature);
  console.log('Signer (hex):', hexSigner);

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .submitSignedRuntimeCall(
      // Solana prefix will be added in the SC previous to signature verification
      hexRuntimeCall,
      {
        nonce: svmNonce,
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
    .send();

  await observeEvent(globalLogger, `environment:SignedRuntimeCallSubmitted`, {
    test: (event) => {
      const matchSerializedCall = event.data.serializedCall === hexRuntimeCall;
      const matchSigner =
        event.data.signerAccountId === 'cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1';
      const dispatchOk = event.data.dispatchResult.Ok !== undefined;
      return matchSerializedCall && matchSigner && dispatchOk;
    },
    historicalCheckBlocks: 1,
  }).event;

  logger.info('Signing and submitting user-signed payload with EVM wallet using personal_sign');

  // EVM Whale -> SC account (`cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`)
  let evmNonce = (await chainflip.rpc.system.accountNextIndex(
    'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7',
  )) as unknown as number;
  const evmPayload = encodeDomainDataToSign(runtimeCall, evmNonce, expiryBlock);
  // Create the Ethereum-prefixed message
  const prefix = `\x19Ethereum Signed Message:\n${evmPayload.length}`;
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), evmPayload]);
  const evmHexPrefixedMessage = '0x' + prefixedMessage.toString('hex');
  console.log('Prefixed Message (hex):', evmHexPrefixedMessage);

  const { privkey: whalePrivKey, pubkey: evmSigner } = getEvmWhaleKeypair('Ethereum');
  const ethWallet = new Wallet(whalePrivKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  if (evmSigner.toLowerCase() !== ethWallet.address.toLowerCase()) {
    throw new Error('Address does not match expected pubkey');
  }

  const evmSignature = await ethWallet.signMessage(evmPayload);

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .submitSignedRuntimeCall(
      // Ethereum prefix will be added in the SC previous to signature verification
      hexRuntimeCall,
      {
        nonce: evmNonce,
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
    .send();

  await observeEvent(globalLogger, `environment:SignedRuntimeCallSubmitted`, {
    test: (event) => {
      const matchSerializedCall = event.data.serializedCall === hexRuntimeCall;
      const matchSigner =
        event.data.signerAccountId === 'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7';
      const dispatchOk = event.data.dispatchResult.Ok !== undefined;
      return matchSerializedCall && matchSigner && dispatchOk;
    },
    historicalCheckBlocks: 1,
  }).event;

  logger.info('Signing and submitting user-signed payload with EVM wallet using EIP-712');

  // EIP-712 signing
  evmNonce = (
    await chainflip.rpc.system.accountNextIndex('cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7')
  ).toNumber();

  const domain = {
    name: chainName,
    // TBD if we need/want this
    version: '0',
  };

  const types = {
    Metadata: [
      { name: 'from', type: 'address' },
      { name: 'nonce', type: 'uint256' },
      { name: 'expiryBlock', type: 'uint256' },
    ],
    // This is just an example.
    SystemRemark: [{ name: 'remark', type: 'bytes[]' }],
    RuntimeCall: [
      { name: 'call', type: 'SystemRemark' },
      { name: 'metadata', type: 'Metadata' },
    ],
  };

  const message = {
    // TODO: Runtime Calls will need to be converted appropriately
    // to an EIP-712 human-readable format.
    call: {
      remark: [], // Empty bytes for system.remark([])
    },
    metadata: {
      from: evmSigner,
      nonce: evmNonce,
      expiryBlock,
    },
  };

  const evmSignatureEip712 = await ethWallet.signTypedData(domain, types, message);
  console.log('EIP712 Signature:', evmSignatureEip712);

  // const encodedPayload = ethers.TypedDataEncoder.encode(domain, types, message);
  // console.log('EIP-712 Encoded Payload:', encodedPayload);
  // const hash = ethers.TypedDataEncoder.hash(domain, types, message);
  // console.log('EIP-712 Hash:', hash);
  // const hashDomain = ethers.TypedDataEncoder.hashDomain(domain);
  // console.log('EIP-712 Domain Hash:', hashDomain);
  // const messageHash = ethers.TypedDataEncoder.from(types).hash(message);
  // console.log('EIP-712 Message Hash:', messageHash);

  // console.log('BorrowData hash:', ethers.TypedDataEncoder.hashStruct('BorrowData', types, message.BorrowAndWithdrawData.Borrow));
  // console.log('WithdrawData hash:', ethers.TypedDataEncoder.hashStruct('WithdrawData', types, message.BorrowAndWithdrawData.Withdraw));
  // console.log('BorrowAndWithdrawData hash:', ethers.TypedDataEncoder.hashStruct('BorrowAndWithdrawData', types, message.BorrowAndWithdrawData));
  // console.log('BorrowAndWithdraw hash:', ethers.TypedDataEncoder.hashStruct('BorrowAndWithdraw', types, message));
  // console.log('TypeScript EIP-712 hash:', hash);

  // // console.log('Borrow hash:', ethers.TypedDataEncoder.hashStruct('Borrow', types, message));

  // await brokerMutex.runExclusive(brokerUri, async () => {
  //   const brokerNonce = await chainflip.rpc.system.accountNextIndex(broker.address);
  //   await chainflip.tx.environment
  //     .submitSignedRuntimeCall(
  //       // The  EIP-712 payload will be build in the State chain previous to signature verification
  //       hexRuntimeCall,
  //       {
  //         nonce,
  //         expiryBlock,
  //       },
  //       {
  //         Ethereum: {
  //           signature: evmSignatureEip712,
  //           signer: evmSigner,
  //           sig_type: 'Eip712',
  //         },
  //       },
  //     )
  //     .signAndSend(broker, { nonce: brokerNonce }, handleSubstrateError(chainflip));
  // });

  // await observeEvent(globalLogger, `environment:SignedRuntimeCallSubmitted`, {
  //   test: (event) => {
  //     const matchSerializedCall = !!event.data.decodedAction.Lending?.Borrow;
  //     const matchSignedPayload = event.data.signedPayload === encodedPayload;
  //     return matchSerializedCall && matchSignedPayload;
  //   },
  //   historicalCheckBlocks: 1,
  // }).event;
}
