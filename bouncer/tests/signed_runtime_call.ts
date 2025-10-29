import { TestContext } from 'shared/utils/test_context';
import { InternalAsset as Asset } from '@chainflip/cli';
import {
  createEvmWallet,
  createStateChainKeypair,
  decodeSolAddress,
  externalChainToScAccount,
  handleSubstrateError,
  newAssetAddress,
  shortChainFromAsset,
  sleep,
} from 'shared/utils';
import { u8aToHex } from '@polkadot/util';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { globalLogger, Logger } from 'shared/utils/logger';
import { fundFlip } from 'shared/fund_flip';
import z from 'zod';
import { ApiPromise } from '@polkadot/api';
import { signBytes, getUtf8Encoder, generateKeyPairSigner } from '@solana/kit';
import { send } from 'shared/send';
import { setupBrokerAccount } from 'shared/setup_account';
import { Enum, Bytes as TsBytes, Option, _void, Tuple } from 'scale-ts';

/// Codecs for the special LP deposit channel opening
const encodedAddressCodec = Enum({
  Eth: TsBytes(20), // [u8; 20]
  Dot: TsBytes(32), // [u8; 32]
  Btc: TsBytes(), // Vec<u8>
  Arb: TsBytes(20), // [u8; 20]
  Sol: TsBytes(32), // [u8; 32]
  Hub: TsBytes(32), // [u8; 32]
});

const accountRoleCodec = Enum({
  Unregistered: _void,
  Broker: _void,
  LiquidityProvider: _void,
  Validator: _void,
});

const remarkDataCodec = Tuple(encodedAddressCodec, Option(accountRoleCodec));

/// EIP-712 payloads schema
const eipPayloadSchema = z.object({
  domain: z.any(),
  types: z.any(),
  message: z.any(),
  primaryType: z.string(), // Some libraries (e.g. wagmi) also require the primaryType
});

const encodedNonNativeCallSchema = z
  .object({
    Eip712: eipPayloadSchema,
  })
  .strict();

const encodedBytesSchema = z
  .object({
    String: z.string(),
  })
  .strict();

const transactionMetadataSchema = z.object({
  nonce: z.number(),
  expiry_block: z.number(),
});

const encodeNonNativeCallResponseSchema = z.tuple([
  z.union([encodedNonNativeCallSchema, encodedBytesSchema]),
  transactionMetadataSchema,
]);

// Default value for number of blocks after which the signed call will expire
const blocksToExpiry = 20;

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

  // EVM to ScAccount e.g. whale wallet -> `cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`
  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);

  logger.info(`Funding with FLIP to register the EVM account: ${evmScAccount}`);
  await fundFlip(logger, evmScAccount, '1000');

  logger.info(`Registering EVM account as operator: ${evmScAccount}`);
  const call = getRegisterOperatorCall(chainflip);
  const hexRuntimeCall = u8aToHex(chainflip.createType('Call', call.method).toU8a());

  const response = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    blocksToExpiry,
    evmScAccount,
    { Eth: 'Eip712' },
  );

  const [eipPayload, transactionMetadata] = encodeNonNativeCallResponseSchema.parse(response);
  logger.debug('eipPayload', JSON.stringify(eipPayload, null, 2));
  logger.debug('transactionMetadata', JSON.stringify(transactionMetadata, null, 2));

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
        transactionMetadata: {
          nonce: transactionMetadata.nonce,
          expiryBlock: transactionMetadata.expiry_block,
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
  // const svmKeypair = await generateKeyPair();
  const svmKeypair = await generateKeyPairSigner();

  // SVM to ScAccount e.g. whale wallet -> `cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1`
  const svmScAccount = externalChainToScAccount(decodeSolAddress(svmKeypair.address.toString()));

  logger.info(`Funding with FLIP to register the SVM account: ${svmScAccount}`);
  await fundFlip(logger, svmScAccount, '1000');

  logger.info(`Registering SVM account as operator: ${svmScAccount}`);
  const call = getRegisterOperatorCall(chainflip);
  const calls = [call];

  const batchCall = chainflip.tx.environment.batch(calls);
  const encodedBatchCall = chainflip.createType('Call', batchCall.method).toU8a();
  const hexBatchRuntimeCall = u8aToHex(encodedBatchCall);

  const svmResponse = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexBatchRuntimeCall,
    blocksToExpiry,
    svmScAccount,
    { Sol: 'Domain' },
  );

  const [svmBytesPayload, svmTransactionMetadata] =
    encodeNonNativeCallResponseSchema.parse(svmResponse);
  logger.debug('SvmBytesPayload', JSON.stringify(svmBytesPayload, null, 2));
  logger.debug('svmTransactionMetadata', JSON.stringify(svmTransactionMetadata, null, 2));

  const svmPayload = encodedBytesSchema.parse(svmBytesPayload);

  // Using Solana Kit instead of the @solana/web3.js because it has a direct
  // method to sign raw bytes.
  const message = getUtf8Encoder().encode(svmPayload.String);
  const signedBytes = await signBytes(svmKeypair.keyPair.privateKey, message);

  const hexSigner = decodeSolAddress(svmKeypair.address);
  const hexSignature = '0x' + Buffer.from(signedBytes).toString('hex');

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .nonNativeSignedCall(
      {
        // Solana prefix will be added in the SC previous to signature verification
        call: hexBatchRuntimeCall,
        transactionMetadata: svmTransactionMetadata,
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

  const personalSignResponse = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    blocksToExpiry,
    evmNonce,
    { Eth: 'PersonalSign' },
  );

  const [evmPayload, personalSignMetadata] =
    encodeNonNativeCallResponseSchema.parse(personalSignResponse);
  logger.debug('evmPayload', JSON.stringify(evmPayload, null, 2));
  logger.debug('personalSignMetadata', JSON.stringify(personalSignMetadata, null, 2));

  const parsedEvmPayload = encodedBytesSchema.parse(evmPayload);
  const evmString = parsedEvmPayload.String;

  if (evmNonce !== personalSignMetadata.nonce) {
    throw new Error(
      `Nonce mismatch: provided ${evmNonce}, metadata has ${personalSignMetadata.nonce}`,
    );
  }

  // Sign with personal_sign (automatically adds prefix)
  const evmSignature = await evmWallet.signMessage(evmString);

  // Submit as unsigned extrinsic - no broker needed
  await chainflip.tx.environment
    .nonNativeSignedCall(
      // Ethereum prefix will be added in the SC previous to signature verification
      {
        call: hexRuntimeCall,
        transactionMetadata: personalSignMetadata,
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

async function testSpecialLpDeposit(logger: Logger, asset: Asset) {
  await using chainflip = await getChainflipApi();

  logger.info('Setting up a broker account');
  const brokerUri = `//BROKER_SPECIAL_DEPOSIT_CHANNEL`;
  const broker = createStateChainKeypair(brokerUri);
  await setupBrokerAccount(logger, brokerUri);

  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);
  const evmNonce = (await chainflip.rpc.system.accountNextIndex(evmScAccount)).toNumber();
  const refundAddress = await newAssetAddress(asset, brokerUri + Math.random() * 100);

  let addressBytes;

  if (asset === 'Btc') {
    // In prod we should encode the BTC with the adequate encoding, this is to keep it simple
    addressBytes = new Uint8Array(Buffer.from(refundAddress, 'utf-8'));
  } else {
    addressBytes = new Uint8Array(Buffer.from(refundAddress.slice(2), 'hex'));
  }

  const remarkData = remarkDataCodec.enc([
    { tag: shortChainFromAsset(asset), value: addressBytes },
    { tag: 'LiquidityProvider', value: undefined },
  ]);

  const call = chainflip.tx.system.remark(Array.from(remarkData));
  const hexRuntimeCall = u8aToHex(chainflip.createType('Call', call.method).toU8a());

  const response = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    blocksToExpiry,
    evmNonce,
    { Eth: 'Eip712' },
  );

  const [eipPayload, transactionMetadata] = encodeNonNativeCallResponseSchema.parse(response);
  const parsedPayload = encodedNonNativeCallSchema.parse(eipPayload);
  const { domain, types, message } = parsedPayload.Eip712;
  delete types.EIP712Domain;

  const evmSignatureEip712 = await evmWallet.signTypedData(domain, types, message);

  const nonce = await chainflip.rpc.system.accountNextIndex(broker.address);
  await chainflip.tx.liquidityProvider
    .requestLiquidityDepositAddressForExternalAccount(
      {
        Ethereum: {
          signature: evmSignatureEip712,
          signer: evmWallet.address,
          sigType: 'Eip712',
        },
      },
      {
        nonce: transactionMetadata.nonce,
        expiryBlock: transactionMetadata.expiry_block,
      },
      asset,
      0,
      { [shortChainFromAsset(asset).toLowerCase()]: refundAddress },
      'LiquidityProvider',
    )
    .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));

  logger.info('Opening special deposit channel and depositing..');

  const eventResult = await observeEvent(
    logger,
    'liquidityProvider:AccountCreationDepositAddressReady',
    {
      test: (event) =>
        event.data.requesterId === broker.address && event.data.accountId === evmScAccount,
    },
  ).event;
  const depositAddress = eventResult.data.depositAddress[shortChainFromAsset(asset)];

  await send(logger, asset, depositAddress);

  logger.info('Waiting for FLIP balance to be credited...');

  let attempt = 0;
  // eslint-disable-next-line no-constant-condition
  while (true) {
    const account = (await chainflip.query.flip.account(evmScAccount)).toJSON() as {
      balance: string;
    };
    const balance = BigInt(account.balance);

    if (balance > 0) {
      logger.info('FLIP balance credited successfully');
      break;
    }

    if (attempt >= 10) {
      throw new Error('Timeout waiting for FLIP balance to be credited');
    }
    attempt++;
    await sleep(6000);
  }
}

export async function testSignedRuntimeCall(testContext: TestContext) {
  await Promise.all([
    testEvmEip712(testContext.logger.child({ tag: `EvmSignedCall` })),
    testSvmDomain(testContext.logger.child({ tag: `SvmDomain` })),
    testEvmPersonalSign(testContext.logger.child({ tag: `EvmPersonalSign` })),
    testSpecialLpDeposit(testContext.logger.child({ tag: `SpecialLpDeposit` }), 'Btc'),
    testSpecialLpDeposit(testContext.logger.child({ tag: `SpecialLpDeposit` }), 'Eth'),
  ]);
}
