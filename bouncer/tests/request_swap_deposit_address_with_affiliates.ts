import * as ss58 from '@chainflip/utils/ss58';
import * as base58 from '@chainflip/utils/base58';
import { isHex } from '@chainflip/utils/string';
import { hexToBytes } from '@chainflip/utils/bytes';
import { ApiPromise, Keyring } from '@polkadot/api';
import { broker, chainConstants, getInternalAsset } from '@chainflip/cli';
import assert from 'assert';
import { z } from 'zod';
import { getChainflipApi } from '../shared/utils/substrate';
import { deferredPromise, handleSubstrateError, shortChainFromAsset } from '../shared/utils';
import { ExecutableTest } from '../shared/executable_test';

const keyring = new Keyring({ type: 'sr25519' });
keyring.setSS58Format(2112);

const account = keyring.addFromUri('//BROKER_2');
const account2 = keyring.addFromUri('//BROKER_1');
type NewSwapRequest = Parameters<(typeof broker)['buildExtrinsicPayload']>[0];

const numberSchema = z.string().transform((n) => Number(n.replace(/,/g, '')));
const bigintSchema = z.string().transform((n) => BigInt(n.replace(/,/g, '')));

const shortChainSchema = z.enum(['Btc', 'Eth', 'Arb', 'Dot', 'Sol']);

const addressTransforms = {
  Btc: (address: string) => address,
  Eth: (address: string) => address.toLowerCase(),
  Arb: (address: string) => address.toLowerCase(),
  Dot: (address: string) =>
    isHex(address) ? ss58.encode({ data: address, ss58Format: 0 }) : address,
  Sol: (address: string) => (isHex(address) ? base58.encode(hexToBytes(address)) : address),
} as const;

const eventSchema = z
  .object({
    event: z.object({
      data: z.object({
        destinationAddress: z.record(shortChainSchema, z.string()),
        sourceAsset: z.string(),
        destinationAsset: z.string(),
        brokerCommissionRate: numberSchema,
        channelMetadata: z
          .object({ message: z.string(), gasBudget: bigintSchema, ccmAdditionalData: z.string() })
          .nullable(),
        boostFee: numberSchema,
        affiliateFees: z.array(z.object({ account: z.string(), bps: numberSchema })),
        refundParameters: z
          .object({
            retryDuration: numberSchema,
            refundAddress: z.record(shortChainSchema, z.string()),
            minPrice: bigintSchema,
          })
          .nullable(),
        dcaParameters: z
          .object({ numberOfChunks: numberSchema, chunkInterval: numberSchema })
          .nullable(),
      }),
    }),
  })
  .transform((event) => event.event.data);

const requestSwapDepositAddress = async (
  chainflip: ApiPromise,
  params: NewSwapRequest,
  getNonce: () => number,
) => {
  const deferred = deferredPromise<z.output<typeof eventSchema>>();
  const { promise, resolve, reject } = deferred;

  const eventMatcher = chainflip.events.swapping.SwapDepositAddressReady;

  const unsubscribe = await chainflip.tx.swapping
    .requestSwapDepositAddressWithAffiliates(...broker.buildExtrinsicPayload(params, 'backspin'))
    .signAndSend(account, { nonce: getNonce() }, (result) => {
      if (!result.isInBlock) return;

      if (result.dispatchError) {
        try {
          handleSubstrateError(chainflip, false)(result);
        } catch (e) {
          reject(e as Error);
          return;
        }
      }

      const event = result.events.find((record) => eventMatcher.is(record.event));
      assert(event, 'SwapDepositAddressReady event not found');
      resolve(eventSchema.parse(event.toHuman()));
    });

  const event = await promise.finally(unsubscribe);

  const sourceAsset = getInternalAsset({ chain: params.srcChain, asset: params.srcAsset });
  const destinationAsset = getInternalAsset({ chain: params.destChain, asset: params.destAsset });

  assert.strictEqual(event.sourceAsset, sourceAsset, 'source asset is wrong');
  assert.strictEqual(event.destinationAsset, destinationAsset, 'destination asset is wrong');

  const destChain = shortChainFromAsset(destinationAsset);
  const transformDestAddress = addressTransforms[destChain];

  assert.strictEqual(
    transformDestAddress(event.destinationAddress[destChain]!),
    transformDestAddress(params.destAddress),
    'destination address is wrong',
  );
  assert.strictEqual(event.brokerCommissionRate, params.commissionBps ?? 0);
  assert.strictEqual(event.boostFee, params.maxBoostFeeBps ?? 0);

  if (params.fillOrKillParams) {
    assert.strictEqual(
      event.refundParameters?.minPrice,
      BigInt(params.fillOrKillParams.minPriceX128),
    );
    assert.strictEqual(
      event.refundParameters?.retryDuration,
      params.fillOrKillParams.retryDurationBlocks,
    );
    const sourceChain = shortChainFromAsset(sourceAsset);
    const transformRefundAddress = addressTransforms[sourceChain];
    assert.strictEqual(
      transformRefundAddress(event.refundParameters!.refundAddress[sourceChain]!),
      transformRefundAddress(params.fillOrKillParams.refundAddress),
    );
  }

  if (params.affiliates) {
    assert.deepStrictEqual(
      params.affiliates
        .toSorted((a, b) => a.account.localeCompare(b.account))
        .map((aff) => ({ account: aff.account, bps: aff.commissionBps })),
      event.affiliateFees.toSorted((a, b) => a.account.localeCompare(b.account)),
    );
  }

  if (params.dcaParams) {
    assert.strictEqual(event.dcaParameters?.numberOfChunks, params.dcaParams.numberOfChunks);
    assert.strictEqual(event.dcaParameters.chunkInterval, params.dcaParams.chunkIntervalBlocks);
  }

  if (params.ccmParams) {
    assert.strictEqual(event.channelMetadata?.message, params.ccmParams.message);
    assert.strictEqual(event.channelMetadata.gasBudget, BigInt(params.ccmParams.gasBudget));
    assert.strictEqual(event.channelMetadata.ccmAdditionalData, params.ccmParams.ccmAdditionalData);
  }
};

const addresses = {
  Bitcoin: [
    'n37AcBDN48pKxBR5DK1fSDFnzFuMhGq9wy', // P2PKH
    '2MuBZJLi7VdTJgdTkeBfWFMMVvGUJYW9aAt', // P2SH
    'bcrt1qzhzr5p67ukjyzfmxsh53a8rv3p06nqq0m3md3q', // P2WPKH
    'bcrt1qqazrtdsvdnl8pv6ypz4jv9h7ud0fs5u4tullapvan2amku7eyujsum5wt8', // P2WSH
    'bcrt1plrgeugjy6vsehsythzxy4jg5ga2zdvuf4zwjchynd78ldklacs4q52p6g5', // Taproot
  ],
  Ethereum: ['0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF'],
  Arbitrum: ['0xa56A6be23b6Cf39D9448FF6e897C29c41c8fbDFF'],
  Polkadot: ['1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo'],
  Solana: ['3yKDHJgzS2GbZB9qruoadRYtq8597HZifnRju7fHpdRC'],
} as const;

const entries = Object.entries as <T>(o: T) => [keyof T, T[keyof T]][];

const baseCases = entries(addresses).flatMap(([destChain, addrs]) =>
  addrs.map(
    (destAddress) =>
      ({
        srcAsset: 'FLIP',
        srcChain: 'Ethereum',
        destChain,
        destAsset: chainConstants[destChain].assets[0],
        destAddress,
      }) as NewSwapRequest,
  ),
);

const refundCases = entries(addresses).flatMap(([srcChain, addrs]) =>
  addrs.map(
    (refundAddress) =>
      ({
        destAsset: 'FLIP',
        destChain: 'Ethereum',
        destAddress: addresses.Ethereum[0],
        srcChain,
        srcAsset: chainConstants[srcChain].assets[0],
        fillOrKillParams: {
          refundAddress,
          minPriceX128: '1',
          retryDurationBlocks: 100,
        },
      }) as NewSwapRequest,
  ),
);

const withDca: NewSwapRequest = {
  ...refundCases[0],
  dcaParams: {
    numberOfChunks: 7200,
    chunkIntervalBlocks: 2,
  },
};

const withCommission: NewSwapRequest = {
  ...baseCases[0],
  commissionBps: 100,
};

const withAffiliates: NewSwapRequest = {
  ...baseCases[0],
  affiliates: [{ account: account2.address, commissionBps: 100 }],
};

const withCcm: NewSwapRequest = {
  ...baseCases.find((c) => c.destChain === 'Arbitrum')!,
  ccmParams: {
    message: '0xcafebabe',
    gasBudget: '1000000',
    ccmAdditionalData: '0xdeadbeef',
  },
};

const main = async () => {
  await using api = await getChainflipApi();
  let nonce = (await api.rpc.system.accountNextIndex(account.address)).toJSON() as number;

  const allCases = [...baseCases, ...refundCases, withDca, withCommission, withAffiliates, withCcm];
  const results = await Promise.allSettled(
    allCases.map((params) => requestSwapDepositAddress(api, params, () => nonce++)),
  );

  // optimism 🚀
  let success = true;

  results.forEach((result, i) => {
    if (result.status === 'fulfilled') {
      console.log('✅', `swap channel ${i} opened successfully`);
    } else {
      // realism 😔
      success = false;
      console.error(
        '❌',
        `swap channel ${i} couldn't be opened`,
        (result.reason as Error).message,
        allCases[i],
      );
    }
  });

  if (!success) {
    console.error('Some tests failed');
    process.exit(1);
  }
};

export const depositChannelCreation = new ExecutableTest('Deposit-Channel-Creation', main, 360);
