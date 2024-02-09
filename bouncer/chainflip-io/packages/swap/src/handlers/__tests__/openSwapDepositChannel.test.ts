import axios from 'axios';
import * as broker from '@/shared/broker';
import { environment } from '@/shared/tests/fixtures';
import prisma from '../../client';
import screenAddress from '../../utils/screenAddress';
import openSwapDepositChannel from '../openSwapDepositChannel';

jest.mock('@/shared/broker', () => ({
  requestSwapDepositAddress: jest.fn(),
}));

jest.mock('../../utils/screenAddress', () => ({
  __esModule: true,
  default: jest.fn().mockResolvedValue(false),
}));

jest.mock('axios');

describe(openSwapDepositChannel, () => {
  beforeAll(async () => {
    jest
      .useFakeTimers({ doNotFake: ['nextTick', 'setImmediate'] })
      .setSystemTime(new Date('2022-01-01'));

    await prisma.chainTracking.create({
      data: {
        chain: 'Ethereum',
        height: BigInt('125'),
        blockTrackedAt: new Date('2023-11-09T10:00:00.000Z'),
      },
    });
  });

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel" CASCADE`;
  });

  it('creates channel and stores it in the database', async () => {
    jest.mocked(axios.post).mockResolvedValueOnce({ data: environment() });
    jest.mocked(broker.requestSwapDepositAddress).mockResolvedValueOnce({
      sourceChainExpiryBlock: BigInt('1000'),
      address: 'address',
      channelId: BigInt('888'),
      issuedBlock: 123,
    });

    const result = await openSwapDepositChannel({
      srcAsset: 'FLIP',
      srcChain: 'Ethereum',
      destAsset: 'DOT',
      destChain: 'Polkadot',
      destAddress: '5FAGoHvkBsUMnoD3W95JoVTvT8jgeFpjhFK8W73memyGBcBd',
      expectedDepositAmount: '777',
    });

    expect(result).toEqual({
      depositAddress: 'address',
      brokerCommissionBps: 0,
      estimatedExpiryTime: 1699537125000,
      id: '123-Ethereum-888',
      issuedBlock: 123,
      srcChainExpiryBlock: 1000n,
    });
    expect(
      jest.mocked(broker.requestSwapDepositAddress).mock.calls,
    ).toMatchSnapshot();
    expect(await prisma.swapDepositChannel.findFirst()).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
    });
  });

  it('creates channel with ccmMetadata and stores it in the database', async () => {
    jest.mocked(axios.post).mockResolvedValueOnce({ data: environment() });
    jest.mocked(broker.requestSwapDepositAddress).mockResolvedValueOnce({
      sourceChainExpiryBlock: BigInt('1000'),
      address: 'address',
      channelId: BigInt('909'),
      issuedBlock: 123,
    });

    const result = await openSwapDepositChannel({
      srcAsset: 'FLIP',
      srcChain: 'Ethereum',
      destAsset: 'USDC',
      destChain: 'Ethereum',
      destAddress: '0xFcd3C82b154CB4717Ac98718D0Fd13EEBA3D2754',
      expectedDepositAmount: '10101010',
      ccmMetadata: {
        message: '0xdeadc0de',
        gasBudget: (125000).toString(),
      },
    });

    expect(result).toEqual({
      depositAddress: 'address',
      brokerCommissionBps: 0,
      estimatedExpiryTime: 1699537125000,
      id: '123-Ethereum-909',
      issuedBlock: 123,
      srcChainExpiryBlock: 1000n,
    });
    expect(
      jest.mocked(broker.requestSwapDepositAddress).mock.calls,
    ).toMatchSnapshot();
    expect(await prisma.swapDepositChannel.findFirst()).toMatchSnapshot({
      id: expect.any(BigInt),
      createdAt: expect.any(Date),
    });
  });

  it('rejects sanctioned addresses', async () => {
    jest.mocked(screenAddress).mockResolvedValueOnce(true);

    await expect(
      openSwapDepositChannel({
        srcAsset: 'FLIP',
        srcChain: 'Ethereum',
        destAsset: 'DOT',
        destChain: 'Polkadot',
        destAddress: '5FAGoHvkBsUMnoD3W95JoVTvT8jgeFpjhFK8W73memyGBcBd',
        expectedDepositAmount: '777',
      }),
    ).rejects.toThrow('provided address is sanctioned');
  });
});
