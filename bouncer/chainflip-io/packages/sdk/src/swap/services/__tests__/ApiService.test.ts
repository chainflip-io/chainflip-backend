import axios from 'axios';
import { Assets, ChainflipNetworks, Chains } from '@/shared/enums';
import { QuoteRequest } from '../../types';
import ApiService from '../ApiService';

jest.mock('axios', () => ({
  get: jest.fn(),
  post: jest.fn(),
}));

const env = {
  ingressEgress: {
    minimumDepositAmounts: {
      Ethereum: {
        USDC: 0xf4240n,
        ETH: 0x20f81c5f84000n,
        FLIP: 0xde0b6b3a7640000n,
      },
      Polkadot: { DOT: 0x77359400n },
      Bitcoin: { BTC: 0x5f370n },
    },
    ingressFees: {
      Ethereum: { ETH: 0n, FLIP: 0n, USDC: 0n },
      Polkadot: { DOT: 0n },
      Bitcoin: { BTC: 0n },
    },
    egressFees: {
      Ethereum: { ETH: 0n, FLIP: 0n, USDC: 0n },
      Polkadot: { DOT: 0n },
      Bitcoin: { BTC: 0n },
    },
    minimumEgressAmounts: {
      Ethereum: { ETH: 0x1n, USDC: 0x1n, FLIP: 0x1n },
      Polkadot: { DOT: 0x1n },
      Bitcoin: { BTC: 0x258n },
    },
  },
  swapping: {
    maximumSwapAmounts: {
      Ethereum: {
        USDC: 0xf4240n,
        ETH: null,
        FLIP: null,
      },
      Polkadot: { DOT: 0x77359400n },
      Bitcoin: { BTC: null },
    },
  },
};

describe('ApiService', () => {
  const mockRoute = {
    amount: '10000',
    srcChain: Chains.Bitcoin,
    srcAsset: Assets.BTC,
    destChain: Chains.Ethereum,
    destAsset: Assets.ETH,
  } satisfies QuoteRequest;

  describe(ApiService.getChains, () => {
    it.each([
      ChainflipNetworks.sisyphos,
      ChainflipNetworks.perseverance,
    ] as const)('gets testnet chains (%s)', async (network) => {
      expect(await ApiService.getChains(network)).toMatchSnapshot();
    });

    it('gets mainnet chains', async () => {
      expect(
        await ApiService.getChains(ChainflipNetworks.mainnet),
      ).toMatchSnapshot();
    });
  });

  describe.each(Object.values(ChainflipNetworks))(
    `${ApiService.getAssets.name} (%s)`,
    (network) => {
      it.each(Object.values(Chains))(
        'gets the correct assets for networks (%s)',
        async (chain) => {
          expect(
            await ApiService.getAssets(chain, network, env),
          ).toMatchSnapshot();
        },
      );
    },
  );

  describe(ApiService.getQuote, () => {
    it('gets a quote', async () => {
      const mockedGet = jest.mocked(axios.get);
      mockedGet.mockResolvedValueOnce({
        data: {
          id: 'string',
          intermediateAmount: '1',
          egressAmount: '2',
        },
      });

      const route = await ApiService.getQuote(
        'https://swapperoo.org',
        {
          amount: '10000',
          srcChain: Chains.Bitcoin,
          srcAsset: Assets.BTC,
          destChain: Chains.Ethereum,
          destAsset: Assets.ETH,
        },
        {},
      );

      expect(route).toMatchSnapshot();
      expect(mockedGet.mock.lastCall).toMatchSnapshot();
    });

    it('gets a quote with a broker commission', async () => {
      const mockedGet = jest.mocked(axios.get);
      mockedGet.mockResolvedValueOnce({
        data: {
          id: 'string',
          intermediateAmount: '1',
          egressAmount: '2',
        },
      });

      const route = await ApiService.getQuote(
        'https://swapperoo.org',
        {
          amount: '10000',
          srcChain: Chains.Bitcoin,
          srcAsset: Assets.BTC,
          destChain: Chains.Ethereum,
          destAsset: Assets.ETH,
          brokerCommissionBps: 15,
        },
        {},
      );

      expect(route).toMatchSnapshot();
      expect(mockedGet.mock.lastCall).toMatchSnapshot();
    });

    it('passes the signal to axios', async () => {
      const mockedGet = jest.mocked(axios.get);
      mockedGet.mockResolvedValueOnce({
        data: {
          id: 'string',
          intermediateAmount: '1',
          egressAmount: '2',
        },
      });

      await ApiService.getQuote('https://swapperoo.org', mockRoute, {
        signal: new AbortController().signal,
      });

      expect(mockedGet.mock.lastCall?.[1]?.signal).not.toBeUndefined();
    });
  });

  describe(ApiService.getStatus, () => {
    it('forwards whatever response it gets from the swap service', async () => {
      const mockedGet = jest.mocked(axios.get);
      mockedGet.mockResolvedValueOnce({ data: 'hello darkness' });
      mockedGet.mockResolvedValueOnce({ data: 'my old friend' });

      const statusRequest = { id: 'the id' };

      const status1 = await ApiService.getStatus(
        'https://swapperoo.org',
        statusRequest,
        {},
      );
      expect(status1).toBe('hello darkness');
      const status2 = await ApiService.getStatus(
        'https://swapperoo.org',
        statusRequest,
        {},
      );
      expect(status2).toBe('my old friend');
    });

    it('passes the signal to axios', async () => {
      const mockedGet = jest.mocked(axios.get);
      mockedGet.mockResolvedValueOnce({ data: null });

      await ApiService.getStatus(
        'https://swapperoo.org',
        { id: '' },
        { signal: new AbortController().signal },
      );

      expect(mockedGet.mock.lastCall?.[1]?.signal).not.toBeUndefined();
    });
  });
});
