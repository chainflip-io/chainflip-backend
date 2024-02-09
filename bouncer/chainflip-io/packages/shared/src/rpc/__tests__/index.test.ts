import axios from 'axios';
import {
  fundingEnvironment,
  ingressEgressEnvironment,
  poolsEnvironment,
  swappingEnvironment,
} from '../../tests/fixtures';
import {
  getFundingEnvironment,
  getSwappingEnvironment,
  getIngressEgressEnvironment,
  getPoolsEnvironment,
} from '../index';

jest.mock('axios');

const mockResponse = (data: any) =>
  jest.mocked(axios.post).mockResolvedValueOnce({ data });

describe('getFundingEnvironment', () => {
  it('retrieves the funding environment', async () => {
    const spy = mockResponse(fundingEnvironment());

    expect(await getFundingEnvironment({ network: 'perseverance' })).toEqual({
      redemptionTax: 0x4563918244f40000n,
      minimumFundingAmount: 0x8ac7230489e80000n,
    });
    expect(spy.mock.calls).toMatchSnapshot();
  });
});

describe('getSwappingEnvironment', () => {
  it('retrieves the swapping environment', async () => {
    const spy = mockResponse(
      swappingEnvironment({
        maxSwapAmount: '0x4563918244f40000',
      }),
    );

    expect(await getSwappingEnvironment({ network: 'perseverance' })).toEqual({
      maximumSwapAmounts: {
        Bitcoin: {
          BTC: 0x4563918244f40000n,
        },
        Ethereum: {
          ETH: null,
          FLIP: null,
          USDC: 0x4563918244f40000n,
        },
        Polkadot: {
          DOT: null,
        },
      },
    });
    expect(spy.mock.calls).toMatchSnapshot();
  });
});

describe('getIngressEgressEnvironment', () => {
  it('retrieves the ingress egress environment', async () => {
    const spy = mockResponse(
      ingressEgressEnvironment({
        minDepositAmount: '0x4563918244f40000',
        ingressFee: '0x4563918244f40000',
      }),
    );

    expect(
      await getIngressEgressEnvironment({ network: 'perseverance' }),
    ).toEqual({
      minimumDepositAmounts: {
        Bitcoin: { BTC: 0x4563918244f40000n },
        Ethereum: {
          ETH: 0x4563918244f40000n,
          USDC: 0x4563918244f40000n,
          FLIP: 0x4563918244f40000n,
        },
        Polkadot: { DOT: 0x4563918244f40000n },
      },
      ingressFees: {
        Bitcoin: { BTC: 0x4563918244f40000n },
        Ethereum: {
          ETH: 0x4563918244f40000n,
          USDC: 0x4563918244f40000n,
          FLIP: 0x4563918244f40000n,
        },
        Polkadot: { DOT: 0x4563918244f40000n },
      },
      egressFees: {
        Bitcoin: {
          BTC: 0n,
        },
        Ethereum: {
          ETH: 0n,
          FLIP: 0n,
          USDC: 0n,
        },
        Polkadot: {
          DOT: 0n,
        },
      },
      minimumEgressAmounts: {
        Bitcoin: {
          BTC: 0x258n,
        },
        Ethereum: {
          ETH: 0x1n,
          USDC: 0x1n,
          FLIP: 0x1n,
        },
        Polkadot: {
          DOT: 0x1n,
        },
      },
    });
    expect(spy.mock.calls).toMatchSnapshot();
  });
});

describe('getPoolsEnvironment', () => {
  it('retrieves the pools environment', async () => {
    const spy = mockResponse(poolsEnvironment());

    expect(
      await getPoolsEnvironment({ network: 'perseverance' }),
    ).toMatchSnapshot('pool environment');
    expect(spy.mock.calls).toMatchSnapshot();
  });
});
