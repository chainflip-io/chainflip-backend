import * as crypto from 'crypto';
import { Subject } from 'rxjs';
import { Assets } from '@/shared/enums';

describe('quotes', () => {
  let oldEnv: NodeJS.ProcessEnv;
  let collectQuotes: typeof import('../quotes').collectQuotes = jest.fn();
  let findBestQuote: typeof import('../quotes').findBestQuote = jest.fn();
  let buildQuoteRequest: typeof import('../quotes').buildQuoteRequest =
    jest.fn();

  beforeEach(async () => {
    jest.resetModules();
    oldEnv = process.env;
    ({ collectQuotes, findBestQuote, buildQuoteRequest } = await import(
      '../quotes'
    ));
  });

  afterEach(() => {
    process.env = oldEnv;
  });

  describe(collectQuotes, () => {
    beforeEach(() => {
      jest.useFakeTimers();
    });

    afterEach(() => {
      jest.useRealTimers();
    });

    const quotes$ = new Subject<{ client: string; quote: any }>();

    it('returns an empty array if expectedQuotes is 0', async () => {
      expect(await collectQuotes('id', 0, quotes$)).toEqual([]);
    });

    it('returns an empty array if no quotes are received', async () => {
      const promise = collectQuotes('id', 1, quotes$);
      jest.advanceTimersByTime(1001);
      expect(await promise).toEqual([]);
    });

    it('returns an array of quotes if expectedQuotes is received', async () => {
      const id = crypto.randomUUID();
      const promise = collectQuotes(id, 1, quotes$);
      quotes$.next({ client: 'client', quote: { id } });
      expect(await promise).toEqual([{ id }]);
    });

    it('accepts only the first quote from each client', async () => {
      const id = crypto.randomUUID();
      const promise = collectQuotes(id, 2, quotes$);
      for (let i = 0; i < 2; i += 1) {
        quotes$.next({ client: 'client', quote: { id, index: i } });
      }
      jest.advanceTimersByTime(1001);
      expect(await promise).toEqual([{ id, index: 0 }]);
    });

    it('can be configured with QUOTE_TIMEOUT', async () => {
      jest.resetModules();
      process.env.QUOTE_TIMEOUT = '100';
      ({ collectQuotes } = await import('../quotes'));
      const id = crypto.randomUUID();
      const promise = collectQuotes(id, 1, quotes$);
      jest.advanceTimersByTime(101);
      quotes$.next({ client: 'client', quote: { id } });
      expect(await promise).toEqual([]);
    });

    it('eagerly returns after all expected quotes are received', async () => {
      const id = crypto.randomUUID();
      const promise = collectQuotes(id, 2, quotes$);
      quotes$.next({ client: 'client', quote: { id, value: 1 } });
      quotes$.next({ client: 'client2', quote: { id, value: 2 } });
      // no need to advance timers because setTimeout is never called
      expect(await promise).toEqual([
        { id, value: 1 },
        { id, value: 2 },
      ]);
    });
  });

  describe(findBestQuote, () => {
    it('returns the quote with the highest egressAmount', () => {
      const broker = { egressAmount: '1' };
      const a = { egressAmount: '10' };
      const b = { egressAmount: '20' };
      expect(findBestQuote([a, b], broker)).toBe(b);
      expect(findBestQuote([b, a], broker)).toBe(b);
    });

    it('returns the quote with the highest egressAmount if many match', () => {
      const broker = { egressAmount: '1' };
      const a = { egressAmount: '10' };
      const b = { egressAmount: '20' };
      const c = { egressAmount: '20' };
      expect(findBestQuote([c, a, b], broker)).toBe(c);
      expect(findBestQuote([b, a, c], broker)).toBe(b);
    });

    it("returns the broker quote if it's best", () => {
      const a = { egressAmount: '1' };
      const b = { egressAmount: '10' };
      const broker = { egressAmount: '20' };
      expect(findBestQuote([a, b], broker)).toBe(broker);
    });

    it('returns the broker quote in absence of market maker quotes', () => {
      const broker = { egressAmount: '1' };
      expect(findBestQuote([], broker)).toBe(broker);
    });
  });

  describe(buildQuoteRequest, () => {
    it('returns a QuoteRequest', () => {
      expect(
        buildQuoteRequest({
          srcAsset: Assets.FLIP,
          destAsset: Assets.ETH,
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
          srcAsset: Assets.USDC,
          destAsset: Assets.ETH,
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
          srcAsset: Assets.ETH,
          destAsset: Assets.USDC,
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
