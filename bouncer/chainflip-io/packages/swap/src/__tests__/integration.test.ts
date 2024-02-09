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
import { environment, swapRate } from '@/shared/tests/fixtures';
import prisma from '../client';
import app from '../server';
import { getBrokerQuote } from '../utils/statechain';

jest.mock('../utils/statechain', () => ({
  getBrokerQuote: jest.fn(),
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

const generateKeyPairAsync = promisify(crypto.generateKeyPair);

describe('python integration test', () => {
  jest.setTimeout(10000);

  let privateKey: string;
  const marketMakerId = 'web_team_whales';
  let server: typeof app;
  let child: ChildProcessWithoutNullStreams;
  let stdout$: Observable<string>;
  let serverUrl: string;

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

    const query = {
      srcAsset: Assets.FLIP,
      destAsset: Assets.ETH,
      amount: '1000000000000000000',
    } as QuoteQueryParams;
    const params = new URLSearchParams(query as Record<string, any>);

    jest.mocked(getBrokerQuote).mockResolvedValueOnce({
      id: "doesn't matter",
      intermediateAmount: '2000000000',
      egressAmount: '0', // this shouldn't be the result
    });

    const response = await fetch(`${serverUrl}/quote?${params.toString()}`);

    expect(await response.json()).toEqual({
      id: expect.any(String),
      intermediateAmount: '1996000000',
      egressAmount: '995999999999975000',
      includedFees: [
        {
          amount: '2000000',
          asset: 'FLIP',
          chain: 'Ethereum',
          type: 'INGRESS',
        },
        {
          amount: '1996000',
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
          amount: '3992000',
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
});
