import { Server } from 'http';
import request from 'supertest';
import prisma from '../../client';
import app from '../../server';

describe('server', () => {
  let server: Server;

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE "SwapDepositChannel" CASCADE`;
    server = app.listen(0);
  });

  afterEach((cb) => {
    server.close(cb);
  });

  describe('GET /fees', () => {
    it('gets the fees', async () => {
      expect((await request(app).get('/fees')).body).toMatchInlineSnapshot(`
        {
          "assets": {
            "BTC": "0.0015",
            "DOT": "0.0015",
            "ETH": "0.0015",
            "FLIP": "0.0015",
          },
          "network": "0.001",
        }
      `);
    });
  });
});
