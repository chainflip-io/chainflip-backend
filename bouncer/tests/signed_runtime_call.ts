import { TestContext } from 'shared/utils/test_context';
import {
  decodeSolAddress,
  externalChainToScAccount,
  getEvmEndpoint,
  getEvmWhaleKeypair,
  getSolWhaleKeyPair,
} from 'shared/utils';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { ethers, Wallet } from 'ethers';
import { Struct, u32, str /* bool, Enum, u128, u8 */ } from 'scale-ts';
import { globalLogger } from 'shared/utils/logger';
import { fundFlip } from 'shared/fund_flip';
import z from 'zod';

const eipPayloadSchema = z.object({
  domain: z.any(),
  types: z.any(),
  message: z.any(),
  primaryType: z.string(), // Some libraries (e.g. wagmi) also require the primaryType
});

// Define the schema for EncodedNonNativeCall::Eip712
const encodedNonNativeCallSchema = z
  .object({
    Eip712: eipPayloadSchema,
  })
  .strict();

const encodedBytesSchema = z
  .object({
    Bytes: z.string().regex(/^0x[0-9a-fA-F]*$/, 'Must be a valid hex string'),
  })
  .strict();

const chainName = 'Chainflip-Development';
const version = '0';

export function encodeDomainDataToSign(payload: Uint8Array, nonce: number, blockNumber?: number) {
  const transactionMetadata = TransactionMetadata.enc({
    nonce,
    expiryBlock: blockNumber ?? expiryBlock,
  });
  const chainNameBytes = ChainNameCodec.enc(chainName);
  const versionBytes = VersionCodec.enc(version);
  return new Uint8Array([...payload, ...chainNameBytes, ...versionBytes, ...transactionMetadata]);
}


export const TransactionMetadata = Struct({
  nonce: u32,
  expiryBlock: u32,
});
export const ChainNameCodec = str;
export const VersionCodec = str;

// Default values
const expiryBlock = 10000;

export async function testSignedRuntimeCall(testContext: TestContext) {
  const logger = testContext.logger;
  await using chainflip = await getChainflipApi();

  logger.info('Signing and submitting user-signed payload with EVM wallet using EIP-712');

  const { privkey: whalePrivKey, pubkey: evmSigner } = getEvmWhaleKeypair('Ethereum');
  const ethWallet = new Wallet(whalePrivKey).connect(
    ethers.getDefaultProvider(getEvmEndpoint('Ethereum')),
  );
  if (evmSigner.toLowerCase() !== ethWallet.address.toLowerCase()) {
    throw new Error('Address does not match expected pubkey');
  }

  // EVM Whale -> SC account (`cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`)
  const evmScAccount = externalChainToScAccount(ethWallet.address);

  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(evmScAccount),
  ).replace(/"/g, '');

  // Examples of some calls. Bear in mind that some of these calls will
  // only execute succesfully one time, as after that they will already
  // have a registered role, you then need to deregister. Then doing a
  // different call depending on the current role to avoid failure on
  // consecutive test runs.
  // const call = chainflip.tx.liquidityProvider.registerLpAccount();
  // const call = chainflip.tx.swapping.registerAsBroker();
  let call = chainflip.tx.system.remark([]);

  if (role === 'null') {
    logger.info(`Funding with FLIP to register`);
    // This will be done via a broker deposit channel via a new deposit action - when the user
    // wants to deposit BTC to, for example, borrow USDC, we will open a deposit channel via a
    // broker that will receive the BTC and swap a small amount to FLIP. That will register and
    // fund the account. See PRO-2551.
    await fundFlip(logger, evmScAccount, '1000');
  }

  if (role === 'null' || role === 'Unregistered') {
    logger.info(`Registering as operator`);
    call = chainflip.tx.validator.registerAsOperator(
      {
        feeBps: 2000,
        delegationAcceptance: 'Allow',
      },
      'TestOperator',
    );
  } else if (role === 'Operator') {
    logger.info(`Deregistering as operator`);
    call = chainflip.tx.validator.deregisterAsOperator();
  }

  let evmNonce = (await chainflip.rpc.system.accountNextIndex(evmScAccount)).toNumber();

  const hexRuntimeCall = u8aToHex(chainflip.createType('Call', call.method).toU8a());
  const eipPayload = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    {
      nonce: evmNonce,
      expiry_block: expiryBlock,
    },
    { Eth: 'Eip712' },
  );
  logger.debug('eipPayload', JSON.stringify(eipPayload, null, 2));

  // Parse and validate the response
  const parsedPayload = encodedNonNativeCallSchema.parse(eipPayload);
  const { domain, types, message, primaryType } = parsedPayload.Eip712;
  logger.debug('primaryType:', primaryType);

  // Remove the EIP712Domain from the message to smoothen out differences between Rust and
  // TS's ethers signTypedData. With Wagmi we don't need to remove this. There might be other
  // small conversions that will be needed depending on the exact data that the rpc ends up providing.
  delete types.EIP712Domain;

  const evmSignatureEip712 = await ethWallet.signTypedData(domain, types, message);
  logger.debug('EIP712 Signature:', evmSignatureEip712);

  // Submit to the SC
  await chainflip.tx.environment
    .nonNativeSignedCall(
      {
        call: hexRuntimeCall,
        metadata: {
          nonce: evmNonce,
          expiryBlock,
        },
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

  // Needs to check that the result is not error, as the transaction won't
  // automatically revert/fail as for regular extrinsics.
  await observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    historicalCheckBlocks: 1,
  }).event;

  // return; // Temporary early return to just test the EIP-712

  logger.info('Signing and submitting user-signed payload with Solana wallet');
  const whaleKeypair = getSolWhaleKeyPair();

  // SVM Whale -> SC account (`cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1`)
  const svmScAccount = externalChainToScAccount(
    decodeSolAddress(whaleKeypair.publicKey.toString()),
  );

  if (role === 'null') {
    logger.info(`Funding with FLIP to register`);
    await fundFlip(logger, svmScAccount, '1000');
  } else {
    logger.info(`Account already registered, skipping funding`);
  }

  const remarkCall = chainflip.tx.system.remark([]);
  const calls = [remarkCall];
  // Try a call batch that fails - it will still emit the NonNativeSignedCall event
  // but have an error in the dispatch_result.
  // const calls = [remarkCall, chainflip.tx.validator.forceRotation()];

  const batchCall = chainflip.tx.environment.batch(calls);
  const encodedBatchCall = chainflip.createType('Call', batchCall.method).toU8a();
  const hexBatchRuntimeCall = u8aToHex(encodedBatchCall);

  const svmNonce = (await chainflip.rpc.system.accountNextIndex(svmScAccount)).toNumber();

  const bytesPayload = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexBatchRuntimeCall,
    {
      nonce: svmNonce,
      expiry_block: expiryBlock,
    },
    { Sol: 'Domain' },
  );
  logger.debug('SvmBytesPayload', JSON.stringify(bytesPayload, null, 2));

  const svmPayloadOld = encodeDomainDataToSign(encodedBatchCall, svmNonce);
  const prefixBytes = Buffer.from([0xff, ...Buffer.from('solana offchain', 'utf8')]);
  const solPrefixedMessage = Buffer.concat([prefixBytes, svmPayloadOld]);
  logger.debug("prefixBytes", prefixBytes);
  logger.debug("solPrefixedMessage", solPrefixedMessage);

  // Parse and validate the response
  const svmPayload = encodedBytesSchema.parse(bytesPayload);
  const bytes = svmPayload.Bytes;
  logger.debug('svmBytes', bytes);
  logger.debug("svmBytes", hexToU8a(bytes));

  const signature = sign(hexToU8a(bytes), whaleKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + whaleKeypair.publicKey.toBuffer().toString('hex');

  // Submit as unsigned extrinsic - no broker needed
  // await chainflip.tx.environment
  //   .nonNativeSignedCall(
  //     // Solana prefix will be added in the SC previous to signature verification
  //     {
  //       call: hexBatchRuntimeCall,
  //       metadata: {
  //         nonce: svmNonce,
  //         expiryBlock,
  //       },
  //     },
  //     {
  //       Solana: {
  //         signature: hexSignature,
  //         signer: hexSigner,
  //         sigType: 'Domain',
  //       },
  //     },
  //   )
  //   .send();

  // let nonNativeEvent = observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
  //   historicalCheckBlocks: 1,
  // }).event;

  // let batchCompletedEvent = observeEvent(globalLogger, `environment:BatchCompleted`, {
  //   historicalCheckBlocks: 1,
  // }).event;

  // await Promise.all([nonNativeEvent, batchCompletedEvent]);

  // return;

  logger.info('Signing and submitting user-signed payload with EVM wallet using personal_sign');

  // EVM Whale -> SC account (`cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`)
  evmNonce = (await chainflip.rpc.system.accountNextIndex(evmScAccount)) as unknown as number;
  const evmPayload = encodeDomainDataToSign(encodedBatchCall, evmNonce);
  // Sign with personal_sign (automatically adds prefix)
  const evmSignature = await ethWallet.signMessage(evmPayload);

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .nonNativeSignedCall(
      // Ethereum prefix will be added in the SC previous to signature verification

      {
        call: hexBatchRuntimeCall,
        metadata: {
          nonce: evmNonce,
          expiryBlock,
        },
      },
      {
        Ethereum: {
          signature: evmSignature,
          signer: evmSigner,
          sig_type: 'PersonalSign',
        },
      },
    )
    .send();

  let nonNativeEvent = observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    historicalCheckBlocks: 1,
  }).event;

  let batchCompletedEvent = observeEvent(globalLogger, `environment:BatchCompleted`, {
    historicalCheckBlocks: 1,
  }).event;

  await Promise.all([nonNativeEvent, batchCompletedEvent]);
}

// // Code to manually to try  EIP-712 manual signing to try out encodings manually
// const domainTemp = {
//   name: chainName,
//   version,
// };

// const typesTemp = {
//   Metadata: [
//     { name: 'from', type: 'address' },
//     { name: 'nonce', type: 'uint32' },
//     { name: 'expiryBlock', type: 'uint32' },
//   ],
//   RuntimeCall: [{ name: 'call', type: 'string' }],
//   Transaction: [
//     { name: 'Call', type: 'RuntimeCall' },
//     { name: 'Metadata', type: 'Metadata' },
//   ],
// };

// const messageTemp = {
//   Call: {
//     call: "RuntimeCall::System(Call::remark { remark: [] })",
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
// console.log("EIP-712 Hash uint8Array",  hexToU8a(hashTemp));
// const hashDomainTemp = ethers.TypedDataEncoder.hashDomain(domainTemp);
// console.log('EIP-712 Domain Hash:', hashDomainTemp);
// const messageHashTemp = ethers.TypedDataEncoder.from(typesTemp).hash(messageTemp);
// console.log('EIP-712 Message Hash:', messageHashTemp);

// console.log('Transaction hash:', ethers.TypedDataEncoder.hashStruct('Transaction', typesTemp, messageTemp));
// console.log('RuntimeCall hash:', ethers.TypedDataEncoder.hashStruct('RuntimeCall', typesTemp, messageTemp.Call));
// console.log('Metadata hash:', ethers.TypedDataEncoder.hashStruct('Metadata', typesTemp, messageTemp.Metadata));
// return;
