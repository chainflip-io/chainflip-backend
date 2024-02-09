import * as crypto from 'crypto';
import { once } from 'events';
import { Server } from 'http';
import { AddressInfo } from 'net';
import { Socket, io } from 'socket.io-client';
import request from 'supertest';
import { setTimeout as sleep } from 'timers/promises';
import { promisify } from 'util';
import prisma from '../client';
import app from '../server';

const generateKeyPairAsync = promisify(crypto.generateKeyPair);

describe('server', () => {
  let server: Server;

  beforeEach(() => {
    server = app.listen(0);
  });

  afterEach((cb) => {
    server.close(cb);
  });

  describe('GET /healthcheck', () => {
    it('gets the fees', async () => {
      expect((await request(app).get('/healthcheck')).text).toBe('OK');
    });
  });

  describe('socket.io', () => {
    let socket: Socket;
    const name = 'web_team_whales';
    let privateKey: crypto.KeyObject;

    beforeEach(async () => {
      await prisma.$queryRaw`TRUNCATE TABLE private."MarketMaker" CASCADE`;

      const result = await generateKeyPairAsync('ed25519');
      await prisma.marketMaker.create({
        data: {
          name,
          publicKey: result.publicKey
            .export({ format: 'pem', type: 'spki' })
            .toString(),
        },
      });
      privateKey = result.privateKey;
    });

    afterEach(() => {
      socket.disconnect();
    });

    it('can connect to the server', async () => {
      const { port } = server.address() as AddressInfo;
      const timestamp = Date.now();

      socket = io(`http://localhost:${port}`, {
        auth: {
          client_version: '1',
          market_maker_id: name,
          timestamp,
          signature: crypto
            .sign(null, Buffer.from(`${name}${timestamp}`, 'utf8'), privateKey)
            .toString('base64'),
        },
      });

      const connected = await Promise.race([
        sleep(500).then(() => false),
        once(socket, 'connect').then(() => true),
      ]);

      expect(connected).toBe(true);
    });
  });
});
