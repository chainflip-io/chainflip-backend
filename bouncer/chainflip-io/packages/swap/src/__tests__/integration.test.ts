import { spawn, ChildProcessWithoutNullStreams } from 'child_process';
import * as crypto from 'crypto';
import { on, once } from 'events';
import { AddressInfo } from 'net';
import * as path from 'path';
import {
  Observable,
  filter,
  firstValueFrom,
  from,
  map,
  shareReplay,
  timeout,
} from 'rxjs';
import { promisify } from 'util';
import { Assets } from '@/shared/enums';
import { QuoteQueryParams } from '@/shared/schemas';
import prisma from '../client';
import app from '../server';
import { getBrokerQuote } from '../utils/statechain';

jest.mock('../utils/statechain', () => ({ getBrokerQuote: jest.fn() }));

const generateKeyPairAsync = promisify(crypto.generateKeyPair);

describe('python integration test', () => {
  jest.setTimeout(10000);

  let privateKey: string;
  const marketMakerId = 'web_team_whales';
  let server: typeof app;
  let child: ChildProcessWithoutNullStreams;
  let stdout$: Observable<string>;
  let serverUrl: string;

  beforeEach(async () => {
    await prisma.$queryRaw`TRUNCATE TABLE private."MarketMaker" CASCADE`;
    const pair = await generateKeyPairAsync('ed25519');
    privateKey = pair.privateKey
      .export({ type: 'pkcs8', format: 'pem' })
      .toString();
    await prisma.marketMaker.create({
      data: {
        name: marketMakerId,
        publicKey: pair.publicKey
          .export({ type: 'spki', format: 'pem' })
          .toString(),
      },
    });
    server = app.listen(0);
    serverUrl = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;

    child = spawn('python', [
      path.join(__dirname, '..', '..', 'python-client', 'mock.py'),
      '--private-key',
      privateKey,
      '--market-maker-id',
      marketMakerId,
      '--url',
      serverUrl,
    ]);

    stdout$ = from(on(child.stdout, 'data')).pipe(
      map((buffer) => buffer.toString().trim()),
      shareReplay(),
    );
  });

  afterEach(async () => {
    if (!child.killed) {
      child.kill('SIGINT');
      await once(child, 'exit');
    }
    await promisify(server.close).bind(server)();
  });

  it('replies to a quote request', async () => {
    await expect(
      firstValueFrom(
        stdout$.pipe(
          filter((msg) => msg === 'connected'),
          timeout(10000),
        ),
      ),
    ).resolves.toBe('connected');

    const params = new URLSearchParams({
      srcAsset: Assets.FLIP,
      destAsset: Assets.ETH,
      amount: '1000000000000000000',
    } as QuoteQueryParams);

    jest.mocked(getBrokerQuote).mockResolvedValueOnce({
      id: "doesn't matter",
      intermediateAmount: '2000000000',
      egressAmount: '0', // this shouldn't be the result
    });

    const response = await fetch(`${serverUrl}/quote?${params.toString()}`);

    expect(await response.json()).toEqual({
      id: expect.any(String),
      intermediateAmount: '2000000000',
      egressAmount: '1000000000000000000',
    });
  });
});
