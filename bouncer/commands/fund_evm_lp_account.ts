#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will fund and register an account as LP
//
// For example: ./commands/fund_lp_account.ts //LP_3

import {
  amountToFineAmount,
  assetDecimals,
  createEvmWallet,
  decodeDotAddressForContract,
  decodeSolAddress,
  externalChainToScAccount,
  getEvmEndpoint,
  handleSubstrateError,
  isWithinOnePercent,
  newAssetAddress,
  runWithTimeout,
  runWithTimeoutAndExit,
  shortChainFromAsset,
} from 'shared/utils';
import { globalLogger, Logger } from 'shared/utils/logger';
import { Asset, assetConstants, InternalAsset } from '@chainflip/cli';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { setupLpAccount } from 'shared/setup_account';
import { z } from 'zod';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
import { ApiPromise } from '@polkadot/api';
import { u8aToHex } from '@polkadot/util';
import { getDefaultProvider, HDNodeWallet, Wallet } from 'ethers';
import { send } from 'shared/send';

const args = z.tuple([
  z.any(),
  z.any(),
  z
    .string()
    .transform((val) => JSON.parse(val))
    .refine((val) => Array.isArray(val) && val.length > 0, {
      message: 'EVM mnemonics must be provided',
    }),
]);

const blocksToExpiry = 20;

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

// REGISTER LP
function getRegisterLpCall(chainflip: ApiPromise) {
  return chainflip.tx.liquidityProvider.registerLpAccount();
}

async function observeNonNativeSignedRegisterLpCall(logger: Logger, scAccount: string) {
  const nonNativeSignedCallEvent = observeEvent(globalLogger, `environment:NonNativeSignedCall`, {
    historicalCheckBlocks: 1,
  }).event;

  const accountRoleRegisteredEvent = observeEvent(logger, 'accountRoles:AccountRoleRegistered', {
    test: (event) => event.data.accountId === scAccount && event.data.role === 'LiquidityProvider',
  }).event;

  await Promise.all([nonNativeSignedCallEvent, accountRoleRegisteredEvent]);
}

// REGISTER REFUND ADDRESS
async function getRegisterRefundAddress(chainflip: ApiPromise, ccy: InternalAsset, chain: string) {
  let refundAddress = await newAssetAddress(ccy, 'LP_1');
  refundAddress = chain === 'Hub' ? decodeDotAddressForContract(refundAddress) : refundAddress;
  refundAddress = chain === 'Sol' ? decodeSolAddress(refundAddress) : refundAddress;

  return chainflip.tx.liquidityProvider.registerLiquidityRefundAddress({ [chain]: refundAddress });
}

// OPEN DEPOSIT CHANNEL

function getOpenDepositChannelCall(chainflip: ApiPromise, ccy: InternalAsset) {
  return chainflip.tx.liquidityProvider.requestLiquidityDepositAddress(ccy, null);
}

async function observeNonNativeAccountCredited(
  asset: InternalAsset,
  scAccount: string,
  amount: string,
) {
  await observeEvent(globalLogger, 'assetBalances:AccountCredited', {
    test: (event) =>
      event.data.asset === asset &&
      event.data.accountId === scAccount &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(amountToFineAmount(String(amount), assetDecimals(asset as InternalAsset))),
      ),
    finalized: false,
    timeoutSeconds: 120,
  }).event;
}

async function signCallUsingEvmWallet(
  logger: Logger,
  call: ReturnType<typeof getRegisterLpCall>,
  chainflipApi: ApiPromise,
  evmWallet: HDNodeWallet,
  observeFn?: () => Promise<void>,
) {
  const evmScAccount = externalChainToScAccount(evmWallet.address);
  const evmNonce = (await chainflipApi.rpc.system.accountNextIndex(evmScAccount)).toNumber();
  const hexRuntimeCall = u8aToHex(chainflipApi.createType('Call', call.method).toU8a());

  const personalSignResponse = await chainflipApi.rpc(
    'cf_encode_non_native_call',
    hexRuntimeCall,
    blocksToExpiry,
    evmNonce,
    { Eth: 'PersonalSign' },
  );

  const [evmPayload, personalSignMetadata] =
    encodeNonNativeCallResponseSchema.parse(personalSignResponse);

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
  await chainflipApi.tx.environment
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
  if (observeFn) {
    await observeFn();
  }
}

async function main() {
  const [_, __, mnemonics] = args.parse(process.argv);
  await using chainflipApi = await getChainflipApi();

  for (const mnemonic of mnemonics) {
    const evmWallet = await Wallet.fromPhrase(mnemonic).connect(
      getDefaultProvider(getEvmEndpoint('Ethereum')),
    );
    console.log('Mnemonic: ', evmWallet.mnemonic?.phrase);
    const evmScAccount = externalChainToScAccount(evmWallet.address);

    // globalLogger.info(`Funding with FLIP to register the EVM account: ${evmScAccount}`);
    // await fundFlip(globalLogger, evmScAccount, '1000');

    // // register LP account
    // await signCallUsingEvmWallet(
    //   globalLogger,
    //   getRegisterLpCall(chainflipApi),
    //   chainflipApi,
    //   evmWallet as HDNodeWallet,
    //   () => observeNonNativeSignedRegisterLpCall(globalLogger, evmScAccount),
    // );
    // console.log(`Successfully registered LP account ${evmScAccount}`);

    for (const asset of Object.keys(assetConstants).filter((asset) => asset !== 'Dot')) {
      let amount;
      const chain = shortChainFromAsset(asset as InternalAsset);
      switch (asset) {
        case 'BTC':
          amount = 2;
          break;
        case 'ETH':
          amount = 10;
          break;
        case 'USDC':
          amount = 10000;
          break;
        case 'USDT':
          amount = 10000;
          break;
        case 'SOL':
          amount = 10;
          break;
        default:
          amount = 1000;
          break;
      }

      //   await signCallUsingEvmWallet(
      //     globalLogger,
      //     await getRegisterRefundAddress(chainflipApi, asset as InternalAsset, chain),
      //     chainflipApi,
      //     evmWallet,
      //   );

      await signCallUsingEvmWallet(
        globalLogger,
        getOpenDepositChannelCall(chainflipApi, asset as InternalAsset),
        chainflipApi,
        evmWallet,
      );

      console.log('Waiting for deposit address ready event', evmScAccount, asset);
      const depositAddressReady = await observeEvent(
        globalLogger,
        'liquidityProvider:LiquidityDepositAddressReady',
        {
          test: (event) => event.data.asset === asset && event.data.accountId === evmScAccount,
        },
      ).event;

      const ingressAddress = depositAddressReady.data.depositAddress[chain];

      globalLogger.trace(`Initiating transfer to ${ingressAddress}`);

      await runWithTimeout(
        send(globalLogger, asset as InternalAsset, ingressAddress, String(amount)),
        130,
        globalLogger,
        `sending liquidity ${amount} ${asset}.`,
      );

      await observeNonNativeAccountCredited(asset as InternalAsset, evmScAccount, String(amount));
      globalLogger.debug(`Liquidity deposited to ${ingressAddress}`);
    }
  }
}

// const main = async () => {
//   let i = 0;

//   while (i < 10) {
//     const wallet = await createEvmWallet();
//     console.log(`Mnemonic ${i}: `, wallet.mnemonic?.phrase);
//     console.log('PK: ', wallet.privateKey);
//     console.log('');
//     i++;
//   }
// };
await runWithTimeoutAndExit(main(), 120_000);
