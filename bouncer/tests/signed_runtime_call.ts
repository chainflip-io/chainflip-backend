import { TestContext } from 'shared/utils/test_context';
import { InternalAsset as Asset } from '@chainflip/cli';
import {
  chainFromAsset,
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
import { getChainflipApi } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
import z from 'zod';
import { ApiPromise } from '@polkadot/api';
import { signBytes, getUtf8Encoder, generateKeyPairSigner } from '@solana/kit';
import { send } from 'shared/send';
import { setupBrokerAccount } from 'shared/setup_account';
import { Enum, Bytes as TsBytes } from 'scale-ts';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { environmentNonNativeSignedCall } from 'generated/events/environment/nonNativeSignedCall';
import { accountRolesAccountRoleRegistered } from 'generated/events/accountRoles/accountRoleRegistered';
import { environmentBatchCompleted } from 'generated/events/environment/batchCompleted';
import { swappingAccountCreationDepositAddressReady } from 'generated/events/swapping/accountCreationDepositAddressReady';

/// Codecs for the special LP deposit channel opening
const encodedAddressCodec = Enum({
  Eth: TsBytes(20), // [u8; 20]
  Dot: TsBytes(32), // [u8; 32]
  Btc: TsBytes(), // Vec<u8>
  Arb: TsBytes(20), // [u8; 20]
  Sol: TsBytes(32), // [u8; 32]
  Hub: TsBytes(32), // [u8; 32]
});

const remarkDataCodec = encodedAddressCodec;

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
const blocksToExpiry = 120;

async function observeNonNativeSignedCallAndRole<A = []>(cf: ChainflipIO<A>, scAccount: string) {
  await cf.stepUntilAllEventsOf({
    nonNativeSignedCallEvent: {
      name: 'Environment.NonNativeSignedCall',
      schema: environmentNonNativeSignedCall,
    },
    accountRoleRegistered: {
      name: 'AccountRoles.AccountRoleRegistered',
      schema: accountRolesAccountRoleRegistered.refine(
        (event) => event.accountId === scAccount && event.role === 'Operator',
      ),
    },
  });
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

async function testEvmEip712<A = []>(cf: ChainflipIO<A>) {
  await using chainflip = await getChainflipApi();

  cf.info('Signing and submitting user-signed payload with EVM wallet using EIP-712');

  // EVM to ScAccount e.g. whale wallet -> `cFHsUq1uK5opJudRDd1qkV354mUi9T7FB9SBFv17pVVm2LsU7`
  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);

  cf.info(`Funding with FLIP to register the EVM account: ${evmScAccount}`);
  await fundFlip(cf.logger, evmScAccount, '1000');

  cf.info(`Registering EVM account as operator: ${evmScAccount}`);
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
  cf.debug('eipPayload', JSON.stringify(eipPayload, null, 2));
  cf.debug('transactionMetadata', JSON.stringify(transactionMetadata, null, 2));

  const parsedPayload = encodedNonNativeCallSchema.parse(eipPayload);
  const { domain, types, message, primaryType } = parsedPayload.Eip712;
  cf.debug('primaryType:', primaryType);

  // Remove the EIP712Domain from the message to smoothen out differences between Rust and
  // TS's ethers signTypedData. With Wagmi we don't need to remove this. There might be other
  // small conversions that will be needed depending on the exact data that the rpc ends up providing.
  delete types.EIP712Domain;

  const evmSignatureEip712 = await evmWallet.signTypedData(domain, types, message);
  cf.debug('EIP712 Signature:', evmSignatureEip712);

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

  cf.info('EVM EIP-712 signed call submitted, waiting for events...');
  await observeNonNativeSignedCallAndRole(cf, evmScAccount);
}

// Submit the same call as EVM but using batch to test it out.
async function testSvmDomain<A = []>(cf: ChainflipIO<A>) {
  await using chainflip = await getChainflipApi();

  cf.info('Signing and submitting user-signed payload with Solana wallet');

  // Create a new Solana keypair for each test run to ensure a unique account
  // const svmKeypair = await generateKeyPair();
  const svmKeypair = await generateKeyPairSigner();

  // SVM to ScAccount e.g. whale wallet -> `cFPU9QPPTQBxi12e7Vb63misSkQXG9CnTCAZSgBwqdW4up8W1`
  const svmScAccount = externalChainToScAccount(decodeSolAddress(svmKeypair.address.toString()));

  cf.info(`Funding with FLIP to register the SVM account: ${svmScAccount}`);
  await fundFlip(cf.logger, svmScAccount, '1000');

  cf.info(`Registering SVM account as operator: ${svmScAccount}`);
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
  cf.debug('SvmBytesPayload', JSON.stringify(svmBytesPayload, null, 2));
  cf.debug('svmTransactionMetadata', JSON.stringify(svmTransactionMetadata, null, 2));

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

  cf.info('SVM Domain signed call batch submitted, waiting for events...');

  await cf.all([
    (subcf) => observeNonNativeSignedCallAndRole(subcf, svmScAccount),
    (subcf) => subcf.stepUntilEvent('Environment.BatchCompleted', environmentBatchCompleted),
  ]);
}

async function testEvmPersonalSign<A = []>(cf: ChainflipIO<A>) {
  await using chainflip = await getChainflipApi();

  cf.info('Signing and submitting user-signed payload with EVM wallet using personal_sign');

  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);

  cf.info(`Funding with FLIP to register the EVM account: ${evmScAccount}`);
  await fundFlip(cf.logger, evmScAccount, '1000');

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
  cf.debug('evmPayload', JSON.stringify(evmPayload, null, 2));
  cf.debug('personalSignMetadata', JSON.stringify(personalSignMetadata, null, 2));

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

  cf.info('EVM PersonalSign signed call submitted, waiting for events...');
  await observeNonNativeSignedCallAndRole(cf, evmScAccount);
}

// Testing encoding of a few values in the EIP-712 payload, mainly for u128 and U256
// that can be problematic between Rust and JS big ints. This can be removed once we
// have more extensive tests in PRO-2584.
async function testEvmEip712Encoding<A = []>(cf: ChainflipIO<A>) {
  await using chainflip = await getChainflipApi();

  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);

  const call = chainflip.tx.liquidityProvider.scheduleSwap(
    '1000000000000000000000',
    'Flip',
    'Usdc',
    50,
    {
      maxOraclePriceSlippage: null,
      minPrice: 1000000000000000,
    },
    null,
  );

  const hexRuntimeCall = u8aToHex(chainflip.createType('Call', call.method).toU8a());

  const response = await chainflip.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    blocksToExpiry,
    evmScAccount,
    { Eth: 'Eip712' },
  );

  const [eipPayload, transactionMetadata] = encodeNonNativeCallResponseSchema.parse(response);
  cf.debug('eipPayload', JSON.stringify(eipPayload, null, 2));
  cf.debug('transactionMetadata', JSON.stringify(transactionMetadata, null, 2));

  const parsedPayload = encodedNonNativeCallSchema.parse(eipPayload);
  const { domain, types, message } = parsedPayload.Eip712;
  delete types.EIP712Domain;

  // Overriding with these values make the signing work but then it will fail
  // validation as it is not matching what the SC encodes.
  // message.call.pallet_cf_lp____pallet____Call__schedule_swap__e1c6eb2b_0.amount =
  //   '1000000000000000000000';
  // message.call.pallet_cf_lp____pallet____Call__schedule_swap__e1c6eb2b_0.price_limits.min_price =
  //   '1000000000000000';

  const evmSignatureEip712 = await evmWallet.signTypedData(domain, types, message);

  // If the signing works proceed to fund the account and submit the call to ensure it works.
  // Doing it afterwards to make debugging of the signing faster (not waiting for the funding).
  await fundFlip(cf.logger, evmScAccount, '1000');

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
}

async function testSpecialLpDeposit<A = []>(cf: ChainflipIO<A>, asset: Asset) {
  await using chainflip = await getChainflipApi();

  const initialFlipToBeSentToGateway = (
    await chainflip.query.swapping.flipToBeSentToGateway()
  ).toJSON() as number;

  cf.info('Setting up a broker account');
  const brokerUri = `//BROKER_SPECIAL_DEPOSIT_CHANNEL_${asset}`;
  const broker = createStateChainKeypair(brokerUri);
  await setupBrokerAccount(cf.logger, brokerUri);

  const evmWallet = await createEvmWallet();
  const evmScAccount = externalChainToScAccount(evmWallet.address);
  cf.info('evmScAccount for special LP deposit channel:', evmScAccount);
  const evmNonce = (await chainflip.rpc.system.accountNextIndex(evmScAccount)).toNumber();
  const refundAddress = await newAssetAddress(asset, brokerUri + Math.random() * 100);

  let addressBytes;

  if (asset === 'Btc') {
    // In prod we should encode the BTC with the adequate encoding, this is to keep it simple
    addressBytes = new Uint8Array(Buffer.from(refundAddress, 'utf-8'));
  } else {
    addressBytes = new Uint8Array(Buffer.from(refundAddress.slice(2), 'hex'));
  }

  const remarkData = remarkDataCodec.enc({ tag: shortChainFromAsset(asset), value: addressBytes });

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
  await chainflip.tx.swapping
    .requestAccountCreationDepositAddress(
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
    )
    .signAndSend(broker, { nonce }, handleSubstrateError(chainflip));

  cf.info('Opening special deposit channel and depositing..');

  const eventResult = await cf.stepUntilEvent(
    'Swapping.AccountCreationDepositAddressReady',
    swappingAccountCreationDepositAddressReady.refine(
      (event) =>
        event.requestedBy === broker.address &&
        event.requestedFor === evmScAccount &&
        event.depositAddress.chain === chainFromAsset(asset),
    ),
  );
  const depositAddress = eventResult.depositAddress.address;

  await send(cf.logger, asset, depositAddress);

  cf.info('Waiting for FLIP balance to be credited...');

  let attempt = 0;
  let flipBalanceCredited = false;
  let flipToGatewayIncreased = false;

  // eslint-disable-next-line no-constant-condition
  while (true) {
    // Check FLIP balance if not already credited
    if (!flipBalanceCredited) {
      const account = (await chainflip.query.flip.account(evmScAccount)).toJSON() as {
        balance: string;
      };
      const balance = BigInt(account.balance);

      if (balance > 0) {
        cf.info('FLIP balance credited successfully');
        flipBalanceCredited = true;
      }
    }

    // Check FLIP to be sent to Gateway if not already increased
    if (!flipToGatewayIncreased) {
      const flipToBeSentToGateway = (
        await chainflip.query.swapping.flipToBeSentToGateway()
      ).toJSON() as number;

      if (flipToBeSentToGateway > initialFlipToBeSentToGateway) {
        cf.info('FLIP to be sent to Gateway increased successfully');
        flipToGatewayIncreased = true;
      }
    }

    // Break if both conditions are met
    if (flipBalanceCredited && flipToGatewayIncreased) {
      break;
    }

    if (attempt >= 10) {
      if (!flipBalanceCredited) {
        throw new Error('Timeout waiting for FLIP balance to be credited');
      }
      if (!flipToGatewayIncreased) {
        throw new Error('Timeout waiting for FLIP to be sent to Gateway to increase');
      }
    }
    attempt++;
    await sleep(6000);
  }
}

export async function testSignedRuntimeCall(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await cf.all([
    (subcf) => testEvmEip712(subcf.withChildLogger(`EvmSignedCall`)),
    (subcf) => testSvmDomain(subcf.withChildLogger(`SvmDomain`)),
    (subcf) => testEvmPersonalSign(subcf.withChildLogger(`EvmPersonalSign`)),
    (subcf) => testEvmEip712Encoding(subcf.withChildLogger(`EvmEip712Encoding`)),
    (subcf) => testSpecialLpDeposit(subcf.withChildLogger(`SpecialLpDeposit`), 'Btc'),
    (subcf) => testSpecialLpDeposit(subcf.withChildLogger(`SpecialLpDeposit`), 'Eth'),
  ]);
}
