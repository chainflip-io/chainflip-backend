import { TestContext } from 'shared/utils/test_context';
import { createEvmWallet, decodeSolAddress, externalChainToScAccount } from 'shared/utils';
import { hexToU8a, u8aToHex } from '@polkadot/util';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { sign } from '@solana/web3.js/src/utils/ed25519';
import { Struct, u32, str /* bool, Enum, u128, u8 */ } from 'scale-ts';
import { globalLogger, Logger } from 'shared/utils/logger';
import { fundFlip } from 'shared/fund_flip';
import z from 'zod';
import { Keypair } from '@solana/web3.js';
import { ApiPromise } from '@polkadot/api';

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

export const TransactionMetadata = Struct({
  nonce: u32,
  expiryBlock: u32,
});
export const ChainNameCodec = str;
export const VersionCodec = str;

// Default values
const expiryBlock = 10000;

async function observeNonNativeSignedCallAndRole(logger: Logger, scAccount: string) {
  const nonNativeSignedCallEvent = observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    historicalCheckBlocks: 1,
  }).event;

  const accountRoleRegisteredEvent = observeEvent(logger, 'accountRoles:AccountRoleRegistered', {
    test: (event) => event.data.accountId === scAccount && event.data.role === 'Operator',
  }).event;

  await Promise.all([nonNativeSignedCallEvent, accountRoleRegisteredEvent]);
}

// Using the register operator call as an example of a runtime call to submit.
// Any other runtime call should work as well. E.g.
// chainflip.tx.liquidityProvider.registerLpAccount();
// chainflip.tx.swapping.registerAsBroker();
// chainflip.tx.system.remark([]);
function getRegisterOperatorCall(chainflip: ApiPromise) {
  return chainflip.tx.validator.registerAsOperator(
    {
      feeBps: 2000,
      delegationAcceptance: 'Allow',
    },
    'TestOperator',
  );
}

async function testEvmEip712(logger: Logger) {
  await using chainflip = await getChainflipApi();

  logger.info('Signing and submitting user-signed payload with EVM wallet using EIP-712');

  // EVM Whale -> SC account e.g. whalet -> `cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`)
  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);

  logger.info(`Funding with FLIP to register the EVM account: ${evmScAccount}`);
  await fundFlip(logger, evmScAccount, '1000');

  logger.info(`Registering EVM account as operator`);
  const call = getRegisterOperatorCall(chainflip);
  const hexRuntimeCall = u8aToHex(chainflip.createType('Call', call.method).toU8a());

  const evmNonce = (await chainflip.rpc.system.accountNextIndex(evmScAccount)).toNumber();

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

  const evmSignatureEip712 = await evmWallet.signTypedData(domain, types, message);
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
          signer: evmWallet.address,
          sigType: 'Eip712',
        },
      },
    )
    .send();

  logger.info('EVM EIP-712 signed call submitted, waiting for events...');
  await observeNonNativeSignedCallAndRole(logger, evmScAccount);
}

// Submit the same call as EVM but using batch to test it out.
async function testSvmDomain(logger: Logger) {
  await using chainflip = await getChainflipApi();

  logger.info('Signing and submitting user-signed payload with Solana wallet');

  // Create a new Solana keypair for each test run to ensure a unique account
  const svmKeypair = Keypair.generate();
  logger.debug('Using Solana keypair:', svmKeypair);

  // SVM Whale -> SC account (`cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1`)
  const svmScAccount = externalChainToScAccount(decodeSolAddress(svmKeypair.publicKey.toString()));

  logger.info(`Funding with FLIP to register the SVM account: ${svmScAccount}`);
  await fundFlip(logger, svmScAccount, '1000');

  logger.info(`Registering SVM account as operator`);
  const call = getRegisterOperatorCall(chainflip);
  const calls = [call];
  // To try a call batch that fails we could do something like this:
  // const calls = [call, chainflip.tx.validator.forceRotation()];

  const batchCall = chainflip.tx.environment.batch(calls);
  const encodedBatchCall = chainflip.createType('Call', batchCall.method).toU8a();
  const hexBatchRuntimeCall = u8aToHex(encodedBatchCall);

  const svmNonce = (await chainflip.rpc.system.accountNextIndex(svmScAccount)).toNumber();

  const svmBytesPayload = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexBatchRuntimeCall,
    {
      nonce: svmNonce,
      expiry_block: expiryBlock,
    },
    { Sol: 'Domain' },
  );
  logger.debug('SvmBytesPayload', JSON.stringify(svmBytesPayload, null, 2));

  // Parse and validate the response
  const svmPayload = encodedBytesSchema.parse(svmBytesPayload);
  const svmBytes = svmPayload.Bytes;

  const signature = sign(Buffer.from(hexToU8a(svmBytes)), svmKeypair.secretKey.slice(0, 32));
  const hexSignature = '0x' + Buffer.from(signature).toString('hex');
  const hexSigner = '0x' + svmKeypair.publicKey.toBuffer().toString('hex');

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .nonNativeSignedCall(
      // Solana prefix will be added in the SC previous to signature verification
      {
        call: hexBatchRuntimeCall,
        metadata: {
          nonce: svmNonce,
          expiryBlock,
        },
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

  const events = observeNonNativeSignedCallAndRole(logger, svmScAccount);

  const batchCompletedEvent = observeEvent(globalLogger, `environment:BatchCompleted`, {
    historicalCheckBlocks: 1,
  }).event;

  logger.info('SVM Domain signed call batch submitted, waiting for events...');
  await Promise.all([events, batchCompletedEvent]);
}

async function testEvmPersonalSign(logger: Logger) {
  await using chainflip = await getChainflipApi();

  logger.info('Signing and submitting user-signed payload with EVM wallet using personal_sign');

  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);

  logger.info(`Funding with FLIP to register the EVM account: ${evmScAccount}`);
  await fundFlip(logger, evmScAccount, '1000');

  const evmNonce = (await chainflip.rpc.system.accountNextIndex(evmScAccount)).toNumber();

  const call = getRegisterOperatorCall(chainflip);
  const hexRuntimeCall = u8aToHex(chainflip.createType('Call', call.method).toU8a());

  const evmPayload = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    {
      nonce: evmNonce,
      expiry_block: expiryBlock,
    },
    { Eth: 'PersonalSign' },
  );
  logger.debug('evmPayload', JSON.stringify(evmPayload, null, 2));

  // Parse and validate the response
  const parsedEvmPayload = encodedBytesSchema.parse(evmPayload);
  const evmBytes = parsedEvmPayload.Bytes;

  // Sign with personal_sign (automatically adds prefix)
  const evmSignature = await evmWallet.signMessage(Buffer.from(hexToU8a(evmBytes)));

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .nonNativeSignedCall(
      // Ethereum prefix will be added in the SC previous to signature verification
      {
        call: hexRuntimeCall,
        metadata: {
          nonce: evmNonce,
          expiryBlock,
        },
      },
      {
        Ethereum: {
          signature: evmSignature,
          signer: evmWallet.address,
          sig_type: 'PersonalSign',
        },
      },
    )
    .send();

  logger.info('EVM PersonalSign signed call submitted, waiting for events...');
  await observeNonNativeSignedCallAndRole(logger, evmScAccount);
}

export async function testSignedRuntimeCall(testContext: TestContext) {
  await Promise.all([
    testEvmEip712(testContext.logger.child({ tag: `EvmSignedCall` })),
    testSvmDomain(testContext.logger.child({ tag: `SvmDomain` })),
    testEvmPersonalSign(testContext.logger.child({ tag: `EvmPersonalSign` })),
  ]);
}
