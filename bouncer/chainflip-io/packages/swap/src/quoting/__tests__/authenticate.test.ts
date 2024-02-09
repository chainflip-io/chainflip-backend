import * as crypto from 'crypto';
import { promisify } from 'util';
import prisma from '../../client';
import authenticate from '../authenticate';

const generateKeyPairAsync = promisify(crypto.generateKeyPair);

describe(authenticate, () => {
  let next: jest.Mock;
  let privateKey: crypto.KeyObject;

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE private."MarketMaker" CASCADE`;
    next = jest.fn();
    const pair = await generateKeyPairAsync('ed25519');
    await prisma.marketMaker.create({
      data: {
        name: 'web_team_whales',
        publicKey: pair.publicKey
          .export({ format: 'pem', type: 'spki' })
          .toString('base64'),
      },
    });
    privateKey = pair.privateKey;
  });

  it.each([
    {},
    { client_version: '1' },
    { market_maker_id: 'web_team_whales' },
    { timestamp: Date.now() },
    { signature: 'deadbeef' },
    { client_version: '1', market_maker_id: 'web_team_whales' },
    { client_version: '1', timestamp: Date.now() },
    { client_version: '1', signature: 'deadbeef' },
    { market_maker_id: 'web_team_whales', timestamp: Date.now() },
    { market_maker_id: 'web_team_whales', signature: 'deadbeef' },
    { timestamp: Date.now(), signature: 'deadbeef' },
    {
      client_version: '1',
      market_maker_id: 'web_team_whales',
      timestamp: Date.now(),
    },
  ])('rejects invalid auth shape', async (auth) => {
    await authenticate({ handshake: { auth } } as any, next);
    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(new Error('invalid auth'));
  });

  it.each([[-10001, 1000]])('rejects invalid timestamps', async (diff) => {
    await authenticate(
      {
        handshake: {
          auth: {
            client_version: '1',
            market_maker_id: 'web_team_whales',
            timestamp: Date.now() + diff,
            signature: 'deadbeef',
          },
        },
      } as any,
      next,
    );
    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(new Error('invalid timestamp'));
  });

  it('rejects unknown market maker', async () => {
    await authenticate(
      {
        handshake: {
          auth: {
            client_version: '1',
            market_maker_id: 'unknown',
            timestamp: Date.now(),
            signature: 'deadbeef',
          },
        },
      } as any,
      next,
    );
    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(new Error('market maker not found'));
  });

  it('rejects invalid public key', async () => {
    await prisma.marketMaker.update({
      where: { name: 'web_team_whales' },
      data: { publicKey: 'invalid' },
    });

    await authenticate(
      {
        handshake: {
          auth: {
            client_version: '1',
            market_maker_id: 'web_team_whales',
            timestamp: Date.now(),
            // test different lengths
            signature: 'deadbeef',
          },
        },
      } as any,
      next,
    );

    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(new Error('invalid public key'));
  });

  it.each([
    {
      client_version: '1',
      market_maker_id: 'web_team_whales',
      timestamp: Date.now(),
      // test different lengths
      signature: 'deadbeef',
    },
    {
      client_version: '1',
      market_maker_id: 'web_team_whales',
      timestamp: Date.now(),
      // test same lengths
      signature: 'deadbeefdeadbeefdeadbeefdeadbeef',
    },
  ])('rejects invalid signature', async (auth) => {
    await authenticate(
      {
        handshake: {
          auth,
        },
      } as any,
      next,
    );
    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith(new Error('invalid signature'));
  });

  it('accepts valid authentication', async () => {
    const timestamp = Date.now();
    const name = 'web_team_whales';

    const signature = crypto
      .sign(null, Buffer.from(`${name}${timestamp}`, 'utf8'), privateKey)
      .toString('base64');

    await authenticate(
      {
        handshake: {
          auth: {
            client_version: '1',
            market_maker_id: name,
            timestamp,
            signature,
          },
        },
      } as any,
      next,
    );
    expect(next).toHaveBeenCalledTimes(1);
    expect(next).toHaveBeenCalledWith();
  });
});
