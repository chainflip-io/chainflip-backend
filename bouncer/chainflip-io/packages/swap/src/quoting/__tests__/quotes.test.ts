import * as crypto from 'crypto';
import { Subject } from 'rxjs';
import { Asset, Assets, assetChains, AssetAndChain } from '@/shared/enums';
import env from '@/swap/config/env';
import { Pool } from '../../client';
import {
  buildQuoteRequest,
  collectMakerQuotes,
  findBestQuote,
  subtractFeesFromMakerQuote,
} from '../quotes';

jest.mock('@/shared/consts', () => ({
  ...jest.requireActual('@/shared/consts'),
  getPoolsNetworkFeeHundredthPips: jest.fn().mockReturnValue(1000),
}));

describe('quotes', () => {
  describe(collectMakerQuotes, () => {
    let oldEnv: typeof env;

    beforeEach(() => {
      oldEnv = { ...env };
      jest.useFakeTimers();
    });

    afterEach(() => {
      Object.assign(env, oldEnv);
      jest.useRealTimers();
      jest.resetModules();
    });

    const quotes$ = new Subject<{ client: string; quote: any }>();

    it('returns an empty array if expectedQuotes is 0', async () => {
      expect(await collectMakerQuotes('id', 0, quotes$)).toEqual([]);
    });

    it('returns an empty array if no quotes are received', async () => {
      const promise = collectMakerQuotes('id', 1, quotes$);
      jest.advanceTimersByTime(1001);
      expect(await promise).toEqual([]);
    });

    it('returns an array of quotes if expectedQuotes is received', async () => {
      const id = crypto.randomUUID();
      const promise = collectMakerQuotes(id, 1, quotes$);
      quotes$.next({ client: 'client', quote: { id } });
      expect(await promise).toEqual([{ id }]);
    });

    it('accepts only the first quote from each client', async () => {
      const id = crypto.randomUUID();
      const promise = collectMakerQuotes(id, 2, quotes$);
      for (let i = 0; i < 2; i += 1) {
        quotes$.next({ client: 'client', quote: { id, index: i } });
      }
      jest.advanceTimersByTime(1001);
      expect(await promise).toEqual([{ id, index: 0 }]);
    });

    it('can be configured with QUOTE_TIMEOUT', async () => {
      env.QUOTE_TIMEOUT = 100;

      const id = crypto.randomUUID();
      const promise = collectMakerQuotes(id, 1, quotes$);
      jest.advanceTimersByTime(101);
      quotes$.next({ client: 'client', quote: { id } });
      expect(await promise).toEqual([]);
    });

    it('eagerly returns after all expected quotes are received', async () => {
      const id = crypto.randomUUID();
      const promise = collectMakerQuotes(id, 2, quotes$);
      quotes$.next({ client: 'client', quote: { id, value: 1 } });
      quotes$.next({ client: 'client2', quote: { id, value: 2 } });
      // no need to advance timers because setTimeout is never called
      expect(await promise).toEqual([
        { id, value: 1 },
        { id, value: 2 },
      ]);
    });
  });

  describe(subtractFeesFromMakerQuote, () => {
    const examplePool: Pool = {
      id: 1,
      baseAsset: 'ETH',
      quoteAsset: 'USDC',
      liquidityFeeHundredthPips: 1000,
    };

    it('subtracts fees from quote without intermediate amount', () => {
      expect(
        subtractFeesFromMakerQuote(
          { id: 'quote-id', egressAmount: (100e18).toString() },
          [examplePool],
        ),
      ).toMatchObject({
        id: 'quote-id',
        egressAmount: (99.8e18).toString(),
      });
    });

    it('subtracts fees from quote with intermediate amount', () => {
      expect(
        subtractFeesFromMakerQuote(
          {
            id: 'quote-id',
            intermediateAmount: (100e6).toString(),
            egressAmount: (100e18).toString(),
          },
          [examplePool, examplePool],
        ),
      ).toMatchObject({
        id: 'quote-id',
        intermediateAmount: (99.8e6).toString(),
        egressAmount: (99.7e18).toString(),
      });
    });
  });

  describe(findBestQuote, () => {
    it('returns the quote with the highest egressAmount', () => {
      const broker = {
        id: '1',
        intermediateAmount: undefined,
        egressAmount: '1',
      };
      const a = { id: '2', egressAmount: '10' };
      const b = { id: '3', egressAmount: '20' };
      expect(findBestQuote([a, b], broker)).toBe(b);
      expect(findBestQuote([b, a], broker)).toBe(b);
    });

    it('returns the quote with the highest egressAmount if many match', () => {
      const broker = {
        id: '1',
        intermediateAmount: undefined,
        egressAmount: '1',
      };
      const a = { id: '2', egressAmount: '10' };
      const b = { id: '3', egressAmount: '20' };
      const c = { id: '4', egressAmount: '20' };
      expect(findBestQuote([c, a, b], broker)).toBe(c);
      expect(findBestQuote([b, a, c], broker)).toBe(b);
    });

    it("returns the broker quote if it's best", () => {
      const a = { id: '1', egressAmount: '1' };
      const b = { id: '2', egressAmount: '10' };
      const broker = {
        id: '3',
        intermediateAmount: undefined,
        egressAmount: '20',
      };
      expect(findBestQuote([a, b], broker)).toBe(broker);
    });

    it('returns the broker quote in absence of market maker quotes', () => {
      const broker = {
        id: '1',
        intermediateAmount: undefined,
        egressAmount: '1',
      };
      expect(findBestQuote([], broker)).toBe(broker);
    });
  });

  const wrapAsset = (asset: Asset) =>
    ({ asset, chain: assetChains[asset] }) as AssetAndChain;

  describe(buildQuoteRequest, () => {
    it('returns a QuoteRequest', () => {
      expect(
        buildQuoteRequest({
          srcAsset: wrapAsset(Assets.FLIP),
          destAsset: wrapAsset(Assets.ETH),
          amount: '1000000000000000000',
        }),
      ).toEqual({
        id: expect.any(String),
        source_asset: 'FLIP',
        intermediate_asset: 'USDC',
        destination_asset: 'ETH',
        deposit_amount: '1000000000000000000',
      });
    });

    it('returns a QuoteRequest with a null intermediate_asset if srcAsset is USDC', () => {
      expect(
        buildQuoteRequest({
          srcAsset: wrapAsset(Assets.USDC),
          destAsset: wrapAsset(Assets.ETH),
          amount: '100000000',
        }),
      ).toEqual({
        id: expect.any(String),
        source_asset: 'USDC',
        intermediate_asset: null,
        destination_asset: 'ETH',
        deposit_amount: '100000000',
      });
    });

    it('returns a QuoteRequest with a null intermediate_asset if destAsset is USDC', () => {
      expect(
        buildQuoteRequest({
          srcAsset: wrapAsset(Assets.ETH),
          destAsset: wrapAsset(Assets.USDC),
          amount: '100000000',
        }),
      ).toEqual({
        id: expect.any(String),
        source_asset: 'ETH',
        intermediate_asset: null,
        destination_asset: 'USDC',
        deposit_amount: '100000000',
      });
    });
  });
});
