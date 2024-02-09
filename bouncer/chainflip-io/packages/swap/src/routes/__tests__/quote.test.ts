import * as crypto from 'crypto';
import { once } from 'events';
import { Server } from 'http';
import { AddressInfo } from 'net';
import request from 'supertest';
import { promisify } from 'util';
import RpcClient from '@/shared/node-apis/RpcClient';
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

      sendRequest() {
        throw new Error('unmocked request');
      }
    },
);

describe('server', () => {
  let server: Server;
  let client: QuotingClient;

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
    it('gets the quote when the broker is best', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          intermediary: (2000e6).toString(),
          output: (1e18).toString(),
        });

      const params = new URLSearchParams({
        srcAsset: 'FLIP',
        destAsset: 'ETH',
        amount: '1',
      });

      client.setQuoteRequestHandler(async (req) => ({
        id: req.id,
        intermediate_amount: (1000e6).toString(),
        egress_amount: (5e17).toString(),
      }));

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(body).toMatchObject({
        id: expect.any(String),
        intermediateAmount: (2000e6).toString(),
        egressAmount: (1e18).toString(),
      });
      expect(sendSpy).toHaveBeenCalledTimes(1);
    });

    it('gets the quote when the market maker is best', async () => {
      const sendSpy = jest
        .spyOn(RpcClient.prototype, 'sendRequest')
        .mockResolvedValueOnce({
          intermediary: (2000e6).toString(),
          output: (1e18).toString(),
        });
      const params = new URLSearchParams({
        srcAsset: 'FLIP',
        destAsset: 'ETH',
        amount: '1',
      });

      client.setQuoteRequestHandler(async (req) => ({
        id: req.id,
        intermediate_amount: (2000e6).toString(),
        egress_amount: (1.1e18).toString(),
      }));

      const { body, status } = await request(server).get(
        `/quote?${params.toString()}`,
      );

      expect(status).toBe(200);
      expect(body).toMatchObject({
        id: expect.any(String),
        intermediateAmount: (2000e6).toString(),
        egressAmount: (1.1e18).toString(),
      });
      expect(sendSpy).toHaveBeenCalledTimes(1);
    });
  });
});
