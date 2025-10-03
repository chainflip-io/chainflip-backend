import { TestContext } from 'shared/utils/test_context';
import { getEvmEndpoint, getEvmWhaleKeypair, getSolWhaleKeyPair } from 'shared/utils';
import { u8aToHex } from '@polkadot/util';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';
import { Struct, u32, str /* bool, Enum, u128, u8 */ } from 'scale-ts';
import { globalLogger } from 'shared/utils/logger';
import { fundFlip } from 'shared/fund_flip';

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
const atomic = false;

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
  const { privkey: whalePrivKey, pubkey: evmSigner } = getEvmWhaleKeypair('Ethereum');
  const ethWallet = new Wallet(whalePrivKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  if (evmSigner.toLowerCase() !== ethWallet.address.toLowerCase()) {
    throw new Error('Address does not match expected pubkey');
  }
  console.log('EVM whale address', ethWallet.address);

  // EIP-712 manual signing to try out encodings manually.
  // const domainTemp = {
  //   name: "Chainflip-Development",
  //   version: '0',
  // };

  // const typesTemp = {
  //   Metadata: [
  //     { name: 'from', type: 'address' },
  //     { name: 'nonce', type: 'uint32' },
  //     { name: 'expiryBlock', type: 'uint32' },
  //   ],
  //   RuntimeCall: [{ name: 'value', type: 'bytes' }],
  //   Transaction: [
  //     { name: 'Call', type: 'RuntimeCall' },
  //     { name: 'Metadata', type: 'Metadata' },
  //   ],
  // };

  // const messageTemp = {
  //   Call: {
  //     value: "0x020b040000042a00",
  //   },
  //   Metadata: {
  //     from: "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266",
  //     nonce: 0,
  //     expiryBlock: 10000,
  //   },
  // };

  // const evmSignatureEip712Temp = await ethWallet.signTypedData(domainTemp, typesTemp, messageTemp);
  // console.log('EIP712 Signature:', evmSignatureEip712Temp);

  // const encodedPayload = ethers.TypedDataEncoder.encode(domainTemp, typesTemp, messageTemp);
  // console.log('EIP-712 Encoded Payload:', encodedPayload);
  // const hashTemp = ethers.TypedDataEncoder.hash(domainTemp, typesTemp, messageTemp);
  // console.log('EIP-712 Hash:', hashTemp);
  // const hashDomain = ethers.TypedDataEncoder.hashDomain(domainTemp);
  // console.log('EIP-712 Domain Hash:', hashDomain);
  // const messageHashTemp = ethers.TypedDataEncoder.from(typesTemp).hash(messageTemp);
  // console.log('EIP-712 Message Hash:', messageHashTemp);

  // console.log('Transaction hash:', ethers.TypedDataEncoder.hashStruct('Transaction', typesTemp, messageTemp));
  // console.log('RuntimeCall hash:', ethers.TypedDataEncoder.hashStruct('RuntimeCall', typesTemp, messageTemp.Call));
  // console.log('Metadata hash:', ethers.TypedDataEncoder.hashStruct('Metadata', typesTemp, messageTemp.Metadata));
  // return;

  const logger = testContext.logger;
  await using chainflip = await getChainflipApi();

  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(
      'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7',
    ),
  ).replace(/"/g, '');

  // This will be done via a broker deposit channel via a new deposit action - when the user
  // wants to deposit BTC to, for example, borrow USDC, we will open a deposit channel via a
  // broker that will receive the BTC and swap a small amount to FLIp. That will register and
  // fund the account.
  if (role === 'null') {
    await fundFlip(logger, 'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7', '1000');
  } else {
    logger.info(`Account already registered, skipping funding`);
  }

  // Examples of some calls. Bear in mind that some of these calls will
  // only execute succesfully one time, as after that they will already
  // have a registered role, you then need to deregister.
  // const call = chainflip.tx.liquidityProvider.registerLpAccount();
  // const call = chainflip.tx.swapping.registerAsBroker();
  // const call = chainflip.tx.validator.deregisterAsOperator();
  const call = chainflip.tx.validator.registerAsOperator(
    {
      feeBps: 2000,
      delegationAcceptance: 'Allow',
    },
    'TestOperator',
  );

  const encodedCall = chainflip.createType('Call', call.method).toU8a();
  const hexRuntimeCall = u8aToHex(encodedCall);

  // EIP-712 signing
  let evmNonce = (
    await chainflip.rpc.system.accountNextIndex('cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7')
  ).toNumber();

  const eipPayload = await chainflip.rpc(
    'cf_eip_data',
    ethWallet.address,
    Array.from(encodedCall),
    {
      nonce: evmNonce,
      expiry_block: 10000,
    },
  );

  // Extract data loosely. To be done in a more strict typechecked method once it's settled.
  const domain = eipPayload.domain;
  const types = eipPayload.types;
  // Some libraries (e.g. wagmi) also require the primaryType (eipPayload.primaryType)
  const message = eipPayload.message;

  // Remove the EIP712Domain from the message to smoothen out differences between Rust and
  // TS's ethers signTypedData. With Wagmi we don't need to remove this. There might be other
  // small conversions that will be needed depending on the exact data that the rpc ends up providing.
  delete types.EIP712Domain;

  const evmSignatureEip712 = await ethWallet.signTypedData(domain, types, message);
  console.log('EIP712 Signature:', evmSignatureEip712);

  // Submit to the SC
  await chainflip.tx.environment
    .nonNativeSignedCall(
      hexRuntimeCall,
      {
        nonce: evmNonce,
        expiryBlock,
      },
      {
        Ethereum: {
          signature: evmSignatureEip712,
          signer: evmSigner,
          sigType: 'Eip712',
        },
      },
    )
    .send();

  await observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    test: (event) => {
      const dispatchResult = event.data.dispatchResult;
      const signerAccountMatch =
        event.data.signerAccount === 'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7';
      if (!signerAccountMatch) {
        return false;
      }
      // Error early as there shouldn't be other calls like this in parallel for this PoC.
      if ('Err' in dispatchResult) {
        throw new Error(
          `NonNativeSignedCall failed for signer ${event.data.signerAccount}, error found in execution`,
        );
      }
      return 'Ok' in dispatchResult;
    },
    historicalCheckBlocks: 1,
  }).event;

  return; // Temporary early return to skip the rest of the test while debugging

  logger.info('Signing and submitting user-signed payload with Solana wallet');

  const whaleKeypair = getSolWhaleKeyPair();
  console.log('Sol whale pubkey', whaleKeypair.publicKey.toBase58());

  const calls = [remarkCall];
  // Try a call batch that fails
  // const calls = [remarkCall, chainflip.tx.validator.forceRotation()];

  const batchCall = chainflip.tx.environment.batch(calls, atomic);
  const batchRuntimeCall = batchCall.method;
  const encodedBatchCall = chainflip.createType('Call', batchRuntimeCall).toU8a();
  const hexBatchRuntimeCall = u8aToHex(encodedBatchCall);
  console.log('hexBatchRuntimeCall', hexBatchRuntimeCall);

  // SVM Whale -> SC account (`cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1`)
  const svmNonce = (await chainflip.rpc.system.accountNextIndex(
    'cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1',
  )) as unknown as number;
  const svmPayload = encodeDomainDataToSign(encodedBatchCall, svmNonce, expiryBlock);
  const svmHexPayload = u8aToHex(svmPayload);

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
    .nonNativeSignedCall(
      // Solana prefix will be added in the SC previous to signature verification
      hexBatchRuntimeCall,
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

  await observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    test: (event) =>
      event.data.signerAccount === 'cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1',
    historicalCheckBlocks: 1,
  }).event;

  await observeEvent(globalLogger, `environment:BatchCompleted`, {
    test: (event) =>
      event.data.signerAccount === 'cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1',
    historicalCheckBlocks: 1,
  }).event;

  logger.info('Signing and submitting user-signed payload with EVM wallet using personal_sign');

  // EVM Whale -> SC account (`cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`)
  evmNonce = (await chainflip.rpc.system.accountNextIndex(
    'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7',
  )) as unknown as number;
  const evmPayload = encodeDomainDataToSign(encodedBatchCall, evmNonce, expiryBlock);
  // Create the Ethereum-prefixed message
  const prefix = `\x19Ethereum Signed Message:\n${evmPayload.length}`;
  const prefixedMessage = Buffer.concat([Buffer.from(prefix, 'utf8'), evmPayload]);
  const evmHexPrefixedMessage = '0x' + prefixedMessage.toString('hex');
  console.log('Prefixed Message (hex):', evmHexPrefixedMessage);

  const evmSignature = await ethWallet.signMessage(evmPayload);

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .nonNativeSignedCall(
      // Ethereum prefix will be added in the SC previous to signature verification
      hexBatchRuntimeCall,
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

  await observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    test: (event) =>
      event.data.signerAccount === 'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7',
    historicalCheckBlocks: 1,
  }).event;

  await observeEvent(globalLogger, `environment:BatchCompleted`, {
    test: (event) =>
      event.data.signerAccount === 'cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7',
    historicalCheckBlocks: 1,
  }).event;

  logger.info('Signing and submitting user-signed payload with EVM wallet using EIP-712');
}
