import axios from 'axios';
import { Assets, ChainflipNetworks, Chains } from '@/shared/enums';
import { QuoteRequest } from '../../types';
import ApiService from '../ApiService';

jest.mock('axios', () => ({
  get: jest.fn(),
  post: jest.fn(),
}));

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
        'gets the correct assets for testnets (%s)',
        async (chain) => {
          expect(await ApiService.getAssets(chain, network)).toMatchSnapshot();
        },
      );
    },
  );

  describe(ApiService.getAssets, () => {
    it.each(Object.values(Chains))(
      'gets the correct assets for mainnets (%s)',
      async (chain) => {
        expect(
          await ApiService.getAssets(chain, ChainflipNetworks.mainnet),
        ).toMatchSnapshot();
      },
    );
  });

  describe(ApiService.getQuote, () => {
    it('gets a route with a quote', async () => {
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

  describe(ApiService.requestDepositAddress, () => {
    it('executes the route and returns the data from the service', async () => {
      const mockedPost = jest.mocked(axios.post);
      const response = {
        id: 'new deposit channel id',
        depositAddress: '0xcafebabe',
      };
      mockedPost.mockResolvedValueOnce({ data: response });

      const depositChannel = await ApiService.requestDepositAddress(
        'https://swapperoo.org',
        {
          ...mockRoute,
          amount: mockRoute.amount,
          destAddress: 'abcdefgh',
        },
        {},
      );
      expect(depositChannel).toEqual({
        ...mockRoute,
        destAddress: 'abcdefgh',
        depositChannelId: response.id,
        depositAddress: response.depositAddress,
      });
    });

    it('passes on the signal', async () => {
      const mockedPost = jest.mocked(axios.post);
      mockedPost.mockResolvedValueOnce({
        data: { id: 'new deposit channel id', depositAddress: '0xcafebabe' },
      });

      await ApiService.requestDepositAddress(
        'https://swapperoo.org',
        {
          ...mockRoute,
          amount: mockRoute.amount,
          destAddress: '',
        },
        {
          signal: new AbortController().signal,
        },
      );
      expect(mockedPost.mock.lastCall?.[2]?.signal).not.toBeUndefined();
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
