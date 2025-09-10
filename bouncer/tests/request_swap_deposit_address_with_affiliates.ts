import * as ss58 from '@chainflip/utils/ss58';
import * as base58 from '@chainflip/utils/base58';
import { isHex } from '@chainflip/utils/string';
import { bytesToHex, hexToBytes } from '@chainflip/utils/bytes';
import { ApiPromise, Keyring } from '@polkadot/api';
import { Asset, broker, chainConstants, getInternalAsset } from '@chainflip/cli';
import assert from 'assert';
import { z } from 'zod';
import { getChainflipApi } from 'shared/utils/substrate';
import { Chain, deferredPromise, handleSubstrateError, shortChainFromAsset } from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';

function toEncodedAddress(chain: Chain, address: string) {
  switch (chain) {
    case 'Arbitrum':
      assert(isHex(address), 'Expected hex-encoded EVM address');
      return { Arb: hexToBytes(address) };
    case 'Ethereum':
      assert(isHex(address), 'Expected hex-encoded EVM address');
      return { Eth: hexToBytes(address) };
    case 'Polkadot':
      return { Dot: isHex(address) ? hexToBytes(address) : ss58.decode(address).data };
    case 'Assethub':
      return { Hub: isHex(address) ? hexToBytes(address) : ss58.decode(address).data };
    case 'Solana':
      return { Sol: isHex(address) ? hexToBytes(address) : base58.decode(address) };
    case 'Bitcoin':
      return { Btc: bytesToHex(new TextEncoder().encode(address)) };
    default:
      throw new Error(`Unsupported chain: ${chain}`);
  }
}

export async function depositChannelCreation(testContext: TestContext) {
  const keyring = new Keyring({ type: 'sr25519' });
  keyring.setSS58Format(2112);

  const account1 = keyring.addFromUri('//BROKER_2');
  const account2 = keyring.addFromUri('//BROKER_1');
  type SwapDetails = Omit<
    Parameters<(typeof broker)['requestSwapDepositAddress']>[0],
    'srcChain' | 'destChain'
  > & {
    srcAsset: { asset: Asset; chain: Chain };
    destAsset: { asset: Asset; chain: Chain };
  };

  const numberSchema = z.string().transform((n) => Number(n.replace(/,/g, '')));
  const bigintSchema = z.string().transform((n) => BigInt(n.replace(/,/g, '')));

  const shortChainSchema = z.enum(['Btc', 'Eth', 'Arb', 'Dot', 'Sol', 'Hub']);

  const addressTransforms = {
    Btc: (address: string) => address,
    Eth: (address: string) => address.toLowerCase(),
    Arb: (address: string) => address.toLowerCase(),
    Dot: (address: string) =>
      isHex(address) ? ss58.encode({ data: address, ss58Format: 0 }) : address,
    Sol: (address: string) => (isHex(address) ? base58.encode(hexToBytes(address)) : address),
    Hub: (address: string) =>
      isHex(address) ? ss58.encode({ data: address, ss58Format: 0 }) : address,
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
            .object({
              message: z.string(),
              gasBudget: bigintSchema,
              ccmAdditionalData: z.union([z.object({ Solana: z.any() }), z.literal('NotRequired')]),
            })
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
    params: SwapDetails,
    getNonce: () => number,
  ) => {
    const deferred = deferredPromise<z.output<typeof eventSchema>>();
    const { promise, resolve, reject } = deferred;

    const eventMatcher = chainflip.events.swapping.SwapDepositAddressReady;

    const unsubscribe = await chainflip.tx.swapping
      .requestSwapDepositAddressWithAffiliates(
        getInternalAsset(params.srcAsset),
        getInternalAsset(params.destAsset),
        toEncodedAddress(params.destAsset.chain, params.destAddress),
        params.commissionBps ?? 0,
        params.ccmParams && {
          message: params.ccmParams.message,
          gas_budget: `0x${BigInt(params.ccmParams.gasBudget).toString(16)}`,
          ccm_additional_data: params.ccmParams.ccmAdditionalData,
        },
        getInternalAsset(params.srcAsset) === 'Btc' ? (params.maxBoostFeeBps ?? 0) : 0,
        (params.affiliates ?? []).map(({ account, commissionBps }) => ({
          account: isHex(account) ? account : bytesToHex(ss58.decode(account).data),
          bps: commissionBps,
        })),
        params.fillOrKillParams && {
          retry_duration: params.fillOrKillParams.retryDurationBlocks,
          refund_address: toEncodedAddress(
            params.srcAsset.chain,
            params.fillOrKillParams.refundAddress,
          ),
          min_price: `0x${BigInt(params.fillOrKillParams.minPriceX128).toString(16)}`,
        },
        params.dcaParams && {
          number_of_chunks: params.dcaParams.numberOfChunks,
          chunk_interval: params.dcaParams.chunkIntervalBlocks,
        },
      )
      .signAndSend(account1, { nonce: getNonce() }, (result) => {
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

    const sourceAsset = getInternalAsset(params.srcAsset);
    const destinationAsset = getInternalAsset(params.destAsset);

    assert.strictEqual(
      event.sourceAsset,
      sourceAsset,
      `Expected source asset to be ${sourceAsset}, but got ${event.sourceAsset}`,
    );
    assert.strictEqual(
      event.destinationAsset,
      destinationAsset,
      `Expected destination asset to be ${destinationAsset}, but got ${event.destinationAsset}`,
    );

    const destChain = shortChainFromAsset(destinationAsset);
    const transformDestAddress = addressTransforms[destChain];

    assert.strictEqual(
      transformDestAddress(event.destinationAddress[destChain]!),
      transformDestAddress(params.destAddress),
      `Expected destination address to be ${transformDestAddress(params.destAddress)}, but got ${event.destinationAddress[destChain]!}`,
    );
    assert.strictEqual(
      event.brokerCommissionRate,
      params.commissionBps ?? 0,
      `Expected broker commission rate to be ${params.commissionBps ?? 0}, but got ${event.brokerCommissionRate}`,
    );
    assert.strictEqual(
      event.boostFee,
      params.maxBoostFeeBps ?? 0,
      `Expected boost fee to be ${params.maxBoostFeeBps ?? 0}, but got ${event.boostFee}`,
    );

    if (params.fillOrKillParams) {
      assert.strictEqual(
        event.refundParameters?.minPrice,
        BigInt(params.fillOrKillParams.minPriceX128),
        `Expected refund parameter minPrice to be ${BigInt(params.fillOrKillParams.minPriceX128)}, but got ${event.refundParameters?.minPrice}`,
      );
      assert.strictEqual(
        event.refundParameters?.retryDuration,
        params.fillOrKillParams.retryDurationBlocks,
        `Expected refund parameter retryDuration to be ${params.fillOrKillParams.retryDurationBlocks}, but got ${event.refundParameters?.retryDuration}`,
      );
      const sourceChain = shortChainFromAsset(sourceAsset);
      const transformRefundAddress = addressTransforms[sourceChain];
      assert.strictEqual(
        transformRefundAddress(event.refundParameters!.refundAddress[sourceChain]!),
        transformRefundAddress(params.fillOrKillParams.refundAddress),
        `Expected refund address to be ${params.fillOrKillParams.refundAddress}, but got ${event.refundParameters!.refundAddress[sourceChain]!}`,
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
      assert.strictEqual(
        event.dcaParameters?.numberOfChunks,
        params.dcaParams.numberOfChunks,
        `Expected DCA parameter numberOfChunks to be ${params.dcaParams.numberOfChunks}, but got ${event.dcaParameters?.numberOfChunks}`,
      );
      assert.strictEqual(
        event.dcaParameters.chunkInterval,
        params.dcaParams.chunkIntervalBlocks,
        `Expected DCA parameter chunkInterval to be ${params.dcaParams.chunkIntervalBlocks}, but got ${event.dcaParameters.chunkInterval}`,
      );
    }

    if (params.ccmParams) {
      assert.strictEqual(
        event.channelMetadata?.message,
        params.ccmParams.message === '0x' ? '' : params.ccmParams.message,
        `Expected CCM parameter message to be ${params.ccmParams.message === '0x' ? '' : params.ccmParams.message}, but got ${event.channelMetadata?.message}`,
      );
      assert.strictEqual(
        event.channelMetadata.gasBudget,
        BigInt(params.ccmParams.gasBudget),
        `Expected CCM parameter gasBudget to be ${BigInt(params.ccmParams.gasBudget)}, but got ${event.channelMetadata.gasBudget}`,
      );
      if (params.ccmParams.ccmAdditionalData === '0x') {
        assert.strictEqual(
          event.channelMetadata.ccmAdditionalData,
          'NotRequired',
          `Expected CCM parameter ccmAdditionalData to be NotRequired, but got ${event.channelMetadata.ccmAdditionalData}`,
        );
      }
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
    Solana: ['3yKDHJgzS2GbZB9qruoadRYtq8597HZifnRju7fHpdRC'],
    Assethub: ['1yMmfLti1k3huRQM2c47WugwonQMqTvQ2GUFxnU7Pcs7xPo'],
  } as const;

  const entries = Object.entries as <T>(o: T) => [keyof T, T[keyof T]][];

  const baseCases = entries(addresses).flatMap(([destChain, addrs]) =>
    addrs.map(
      (destAddress) =>
        ({
          srcAsset: { asset: 'FLIP', chain: 'Ethereum' },
          destAsset: { asset: chainConstants[destChain].assets[0], chain: destChain },
          destAddress,
        }) as SwapDetails,
    ),
  );

  const refundCases = entries(addresses).flatMap(([srcChain, addrs]) =>
    addrs.map(
      (refundAddress) =>
        ({
          destAsset: { asset: 'FLIP', chain: 'Ethereum' },
          destAddress: addresses.Ethereum[0],
          srcAsset: { asset: chainConstants[srcChain].assets[0], chain: srcChain },
          fillOrKillParams: {
            refundAddress,
            minPriceX128: '1',
            retryDurationBlocks: 100,
          },
        }) as SwapDetails,
    ),
  );

  const withDca: SwapDetails = {
    ...refundCases[0],
    dcaParams: {
      numberOfChunks: 7200,
      chunkIntervalBlocks: 2,
    },
  };

  const withCommission: SwapDetails = {
    ...baseCases[0],
    commissionBps: 100,
  };

  const withAffiliates: SwapDetails = {
    ...baseCases[0],
    affiliates: [{ account: account2.address, commissionBps: 100 }],
  };

  const withCcm: SwapDetails = {
    ...baseCases.find((c) => c.destAsset.chain === 'Arbitrum')!,
    ccmParams: {
      message: '0xcafebabe',
      gasBudget: '1000000',
      ccmAdditionalData: '0x',
    },
  };

  await using api = await getChainflipApi();
  let nonce = (await api.rpc.system.accountNextIndex(account1.address)).toJSON() as number;

  const allCases = [...baseCases, ...refundCases, withDca, withCommission, withAffiliates, withCcm];
  const results = await Promise.allSettled(
    allCases.map((params) => requestSwapDepositAddress(api, params, () => nonce++)),
  );

  results.forEach((result, i) => {
    if (result.status === 'fulfilled') {
      testContext.debug(`swap channel ${i} opened successfully`);
    } else {
      throw new Error(`Swap channel ${i} couldn't be opened: ${(result.reason as Error).message}`);
    }
  });
}
