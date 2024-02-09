import axios from 'axios';
import * as crypto from 'crypto';
import { once } from 'events';
import { Server } from 'http';
import { AddressInfo } from 'net';
import request from 'supertest';
import { promisify } from 'util';
import RpcClient from '@/shared/node-apis/RpcClient';
import { environment, swapRate } from '@/shared/tests/fixtures';
import prisma from '../../client';
import QuotingClient from '../../quoting/QuotingClient';
import app from '../../server';

const generateKeyPairAsync = promisify(crypto.generateKeyPair);

jest.mock(
  '@/shared/node-apis/RpcClient',
  () =>
    class {
      async connect() {
        return this;
      }

      sendRequest(method: string) {
        throw new Error(`unmocked request: "${method}"`);
      }
    },
);

jest.mock('@/shared/consts', () => ({
  ...jest.requireActual('@/shared/consts'),
  getPoolsNetworkFeeHundredthPips: jest.fn().mockReturnValue(1000),
}));

jest.mock('axios', () => ({
  post: jest.fn((url, data) => {
    if (data.method === 'cf_environment') {
      return Promise.resolve({
        data: environment({
          maxSwapAmount: null,
          ingressFee: '0xF4240', // 2000000
          egressFee: '0x61A8', // 25000
        }),
      });
    }

    if (data.method === 'cf_swap_rate') {
      return Promise.resolve({
        data: swapRate({
          output: `0x${(BigInt(data.params[2]) * 2n).toString(16)}`,
        }),
      });
    }

    throw new Error(`unexpected axios call to ${url}: ${JSON.stringify(data)}`);
  }),
}));

describe('server', () => {
  let server: Server;
  let client: QuotingClient;

  beforeAll(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE public."Pool" CASCADE`;
    await prisma.pool.createMany({
      data: [
        {
          baseAsset: 'FLIP',
          quoteAsset: 'USDC',
          liquidityFeeHundredthPips: 1000,
        },
        {
          baseAsset: 'ETH',
          quoteAsset: 'USDC',
          liquidityFeeHundredthPips: 2000,
        },
      ],
    });
  });

  beforeEach(async () => {
    server = app.listen(0);
    await prisma.$queryRaw`TRUNCATE TABLE private."MarketMaker" CASCADE`;
    const name = 'web_team_whales';
    const pair = await generateKeyPairAsync('ed25519');
    await prisma.marketMaker.create({
      data: {
        name: 'web_team_whales',
        publicKey: pair.publicKey
          .export({ format: 'pem', type: 'spki' })
          .toString('base64'),
      },
    });

    client = new QuotingClient(
      `http://localhost:${(server.address() as AddressInfo).port}`,
      name,
      pair.privateKey.export({ format: 'pem', type: 'pkcs8' }).toString(),
    );
    await once(client, 'connected');
  });

  afterEach((cb) => {
    client.close();
    server.close(cb);
  });

  describe('GET /quote', () => {
    it('rejects if amount is lower than minimum swap amount', async () => {
      jest.mocked(axios.post).mockResolvedValueOnce({
        data: environment({ minDepositAmount: '0xffffff' }),
      });

      const params = new URLSearchParams({
        srcAsset: 'FLIP',
        destAsset: 'ETH',
        amount: '50',
      });

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'expected amount is below minimum swap amount (16777215)',
      });
    });

    it('rejects if amount is higher than maximum swap amount', async () => {
      jest
        .mocked(axios.post)
        .mockResolvedValueOnce({ data: environment({ maxSwapAmount: '0x1' }) });

      const params = new URLSearchParams({
        srcAsset: 'USDC',
        destAsset: 'FLIP',
        amount: '50',
      });

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'expected amount is above maximum swap amount (1)',
      });
    });

    it('gets the quote from usdc when the ingress amount is smaller than the ingress fee', async () => {
      const params = new URLSearchParams({
        srcAsset: 'USDC',
        destAsset: 'ETH',
        amount: (1000).toString(),
      });

      const quoteHandler = jest.fn(async (req) => ({
        id: req.id,
        egress_amount: '0',
      }));
      client.setQuoteRequestHandler(quoteHandler);

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'amount is lower than estimated ingress fee (2000000)',
      });
    });

    it('rejects when the egress amount is smaller than the egress fee', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          egressAmount: (1250).toString(),
        });

      const params = new URLSearchParams({
        srcAsset: 'USDC',
        destAsset: 'ETH',
        amount: (100e6).toString(),
      });

      const quoteHandler = jest.fn(async (req) => ({
        id: req.id,
        egress_amount: '0',
      }));
      client.setQuoteRequestHandler(quoteHandler);

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(400);
      expect(body).toMatchObject({
        message: 'egress amount (0) is lower than minimum egress amount (1)',
      });
      expect(sendSpy).toHaveBeenCalledTimes(1);
    });

    it('gets the quote from usdc with a broker commission', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          egressAmount: (1e18).toString(),
        });

      const params = new URLSearchParams({
        srcAsset: 'USDC',
        destAsset: 'ETH',
        amount: (100e6).toString(),
        brokerCommissionBps: '10',
      });

      const quoteHandler = jest.fn(async (req) => ({
        id: req.id,
        egress_amount: (0.5e18).toString(),
      }));
      client.setQuoteRequestHandler(quoteHandler);

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(quoteHandler).toHaveBeenCalledWith({
        deposit_amount: '97902000', // deposit amount - ingress fee - broker fee
        destination_asset: 'ETH',
        id: expect.any(String),
        intermediate_asset: null,
        source_asset: 'USDC',
      });
      expect(sendSpy).toHaveBeenCalledWith(
        'swap_rate',
        { asset: 'USDC', chain: 'Ethereum' },
        { asset: 'ETH', chain: 'Ethereum' },
        '97902000', // deposit amount - ingress fee - broker fee
      );
      expect(body).toMatchObject({
        id: expect.any(String),
        egressAmount: (1e18 - 25000).toString(),
        includedFees: [
          {
            amount: '2000000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'INGRESS',
          },
          {
            amount: '98000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'BROKER',
          },
          {
            amount: '97902',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'NETWORK',
          },
          {
            amount: '195804',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '25000',
            asset: 'ETH',
            chain: 'Ethereum',
            type: 'EGRESS',
          },
        ],
      });
    });

    it('gets the quote from usdc when the broker is best', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          egressAmount: (1e18).toString(),
        });

      const params = new URLSearchParams({
        srcAsset: 'USDC',
        destAsset: 'ETH',
        amount: (100e6).toString(),
      });

      const quoteHandler = jest.fn(async (req) => ({
        id: req.id,
        egress_amount: (0.5e18).toString(),
      }));
      client.setQuoteRequestHandler(quoteHandler);

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(quoteHandler).toHaveBeenCalledWith({
        deposit_amount: '98000000', // deposit amount - ingress fee
        destination_asset: 'ETH',
        id: expect.any(String),
        intermediate_asset: null,
        source_asset: 'USDC',
      });
      expect(sendSpy).toHaveBeenCalledWith(
        'swap_rate',
        { asset: 'USDC', chain: 'Ethereum' },
        { asset: 'ETH', chain: 'Ethereum' },
        '98000000', // deposit amount - ingress fee
      );
      expect(body).toMatchObject({
        id: expect.any(String),
        egressAmount: (1e18 - 25000).toString(),
        includedFees: [
          {
            amount: '2000000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'INGRESS',
          },
          {
            amount: '98000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'NETWORK',
          },
          {
            amount: '196000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '25000',
            asset: 'ETH',
            chain: 'Ethereum',
            type: 'EGRESS',
          },
        ],
      });
    });

    it('gets the quote to usdc when the broker is best', async () => {
      const env = environment({
        maxSwapAmount: null,
        ingressFee: '0x61A8',
        egressFee: '0x0',
      });

      // method is called three times
      jest
        .mocked(axios.post)
        .mockResolvedValueOnce({ data: env })
        .mockResolvedValueOnce({ data: env })
        .mockResolvedValueOnce({ data: env });

      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          egressAmount: (100e6).toString(),
        });

      const params = new URLSearchParams({
        srcAsset: 'ETH',
        destAsset: 'USDC',
        amount: (1e18).toString(),
      });

      client.setQuoteRequestHandler(async (req) => ({
        id: req.id,
        egress_amount: (50e6).toString(),
      }));

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(body).toMatchObject({
        id: expect.any(String),
        egressAmount: (100e6).toString(),
        includedFees: [
          {
            amount: '25000',
            asset: 'ETH',
            chain: 'Ethereum',
            type: 'INGRESS',
          },
          {
            amount: '100100',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'NETWORK',
          },
          {
            amount: '1999999999999950',
            asset: 'ETH',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '0',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'EGRESS',
          },
        ],
      });
      expect(sendSpy).toHaveBeenCalledTimes(1);
    });

    it('gets the quote with intermediate amount when the broker is best', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          intermediateAmount: (2000e6).toString(),
          egressAmount: (1e18).toString(),
        });

      const params = new URLSearchParams({
        srcAsset: 'FLIP',
        destAsset: 'ETH',
        amount: (1e18).toString(),
      });

      client.setQuoteRequestHandler(async (req) => ({
        id: req.id,
        intermediate_amount: (1000e6).toString(),
        egress_amount: (0.5e18).toString(),
      }));

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(body).toMatchObject({
        id: expect.any(String),
        intermediateAmount: (2000e6).toString(),
        egressAmount: (1e18 - 25000).toString(),
        includedFees: [
          {
            amount: '2000000',
            asset: 'FLIP',
            chain: 'Ethereum',
            type: 'INGRESS',
          },
          {
            amount: '2000000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'NETWORK',
          },
          {
            amount: '999999999998000',
            asset: 'FLIP',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '4000000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '25000',
            asset: 'ETH',
            chain: 'Ethereum',
            type: 'EGRESS',
          },
        ],
      });
      expect(sendSpy).toHaveBeenCalledTimes(1);
    });

    it('gets the quote when the market maker is best', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          intermediateAmount: (2000e6).toString(),
          egressAmount: (1e18).toString(),
        });
      const params = new URLSearchParams({
        srcAsset: 'FLIP',
        destAsset: 'ETH',
        amount: (1e18).toString(),
      });

      client.setQuoteRequestHandler(async (req) => ({
        id: req.id,
        intermediate_amount: (3000e6).toString(),
        egress_amount: (2e18).toString(),
      }));

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(body).toMatchObject({
        id: expect.any(String),
        intermediateAmount: (2994e6).toString(),
        egressAmount: (1.992e18 - 25000).toString(),
        includedFees: [
          {
            amount: '2000000',
            asset: 'FLIP',
            chain: 'Ethereum',
            type: 'INGRESS',
          },
          {
            amount: '2994000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'NETWORK',
          },
          {
            amount: '999999999998000',
            asset: 'FLIP',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '5988000',
            asset: 'USDC',
            chain: 'Ethereum',
            type: 'LIQUIDITY',
          },
          {
            amount: '25000',
            asset: 'ETH',
            chain: 'Ethereum',
            type: 'EGRESS',
          },
        ],
      });
      expect(sendSpy).toHaveBeenCalledTimes(1);
    });
  });
});
