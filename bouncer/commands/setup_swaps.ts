// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: pnpm tsx ./commands/setup_polkadot_vault.ts

import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { exec } from 'child_process';
import { Mutex } from 'async-mutex';
import type { KeyringPair } from '@polkadot/keyring/types';
import { runWithTimeout, sleep } from '../shared/utils';

const deposits = {
  dot: 10000,
  eth: 100,
  btc: 10,
  usdc: 1000000,
} as const;

const values = {
  dot: 10,
  eth: 1000,
  btc: 10000,
} as const;

const decimals = {
  dot: 10,
  eth: 18,
  btc: 8,
  usdc: 6,
} as const;

const chain = {
  dot: 'dot',
  btc: 'btc',
  eth: 'eth',
  usdc: 'eth',
} as const;

const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
let chainflip: ApiPromise;
let keyring: Keyring;
let snowwhite: KeyringPair;
let lp: KeyringPair;
const mutex = new Mutex();

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function observeEvent(eventName: string, dataCheck: (data: any) => boolean): Promise<any> {
  let result;
  let waiting = true;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const unsubscribe: any = await chainflip.query.system.events((events: any[]) => {
    events.forEach((record) => {
      const { event } = record;
      if (event.section === eventName.split(':')[0] && event.method === eventName.split(':')[1]) {
        if (dataCheck(event.data)) {
          result = event.data;
          waiting = false;
          unsubscribe();
        }
      }
    });
  });
  while (waiting) {
    await sleep(1000);
  }
  return result;
}

async function setupCurrency(ccy: keyof typeof chain): Promise<void> {
  console.log('Requesting ' + ccy + ' deposit address');
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy)
      .signAndSend(lp, { nonce: -1 });
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const checkCcy = (data: any): boolean => {
    const result = data[1].toJSON()[chain[ccy]];
    return result !== null && result !== undefined;
  };

  const ingressKey = (
    await observeEvent('liquidityProvider:LiquidityDepositAddressReady', checkCcy)
  )[1].toJSON()[chain[ccy]] as string;
  let ingressAddress = ingressKey;
  if (ccy === 'btc') {
    ingressAddress = '';
    for (let n = 2; n < ingressKey.length; n += 2) {
      ingressAddress += String.fromCharCode(parseInt(ingressKey.substr(n, 2), 16));
    }
  }
  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  exec(
    'pnpm tsx ./commands/fund_' + ccy + '.ts' + ' ' + ingressAddress + ' ' + deposits[ccy],
    { timeout: 30000 },
    (err, stdout, stderr) => {
      if (stderr !== '') process.stdout.write(stderr);
      if (err !== null) {
        console.error(err);
        process.exit(1);
      }
      if (stdout !== '') process.stdout.write(stdout);
    },
  );
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const checkDeposit = (data: any): boolean => data.asset.toJSON().toLowerCase() === ccy;

  await observeEvent('liquidityProvider:AccountCredited', checkDeposit);
  if (ccy === 'usdc') {
    return;
  }
  const price = BigInt(
    Math.round(Math.sqrt(values[ccy] / 10 ** (decimals[ccy] - decimals.usdc)) * 2 ** 96),
  );
  console.log('Setting up ' + ccy + ' pool');
  await mutex.runExclusive(async () => {
    await chainflip.tx.governance
      .proposeGovernanceExtrinsic(chainflip.tx.liquidityPools.newPool(ccy, 100, price))
      .signAndSend(snowwhite, { nonce: -1 });
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const checkPool = (data: any): boolean => data.unstableAsset.toJSON().toLowerCase() === ccy;

  await observeEvent('liquidityPools:NewPoolCreated', checkPool);
  const priceTick = Math.round(
    Math.log(Math.sqrt(values[ccy] / 10 ** (decimals[ccy] - decimals.usdc))) /
    Math.log(Math.sqrt(1.0001)),
  );
  const buyPosition = deposits[ccy] * values[ccy] * 1000000;
  console.log(
    'Placing Buy Limit order for ' + deposits[ccy] + ' ' + ccy + ' at ' + values[ccy] + ' USDC.',
  );
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .collectAndMintLimitOrder(ccy, 'Buy', priceTick, buyPosition)
      .signAndSend(lp, { nonce: -1 }, ({ status, dispatchError }) => {
        if (dispatchError !== undefined) {
          if (dispatchError.isModule) {
            const decoded = chainflip.registry.findMetaError(dispatchError.asModule);
            const { docs, name, section } = decoded;
            console.log(
              `Placing Buy Limit order for ${ccy} failed: ${section}.${name}: ${docs.join(' ')}`,
            );
          } else {
            console.log(
              `Placing Buy Limit order for ${ccy} failed: Error: ` + dispatchError.toString(),
            );
          }
          process.exit(-1);
        }
      });
  });
  console.log(
    'Placing Sell Limit order for ' + deposits[ccy] + ' ' + ccy + ' at ' + values[ccy] + ' USDC.',
  );
  const sellPosition = BigInt(deposits[ccy] * 10 ** decimals[ccy]);
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .collectAndMintLimitOrder(ccy, 'Sell', priceTick, sellPosition)
      .signAndSend(lp, { nonce: -1 }, ({ status, dispatchError }) => {
        if (dispatchError !== undefined) {
          if (dispatchError.isModule) {
            const decoded = chainflip.registry.findMetaError(dispatchError.asModule);
            const { docs, name, section } = decoded;
            console.log(
              `Placing Sell Limit order for ${ccy} failed:${section}.${name}: ${docs.join(' ')}`,
            );
          } else {
            console.log(
              `Placing Sell Limit order for ${ccy} failed: Error: ` + dispatchError.toString(),
            );
          }
          process.exit(-1);
        }
      });
  });
}

async function main(): Promise<void> {
  chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
  });
  await cryptoWaitReady();

  keyring = new Keyring({ type: 'sr25519' });
  const snowwhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
  snowwhite = keyring.createFromUri(snowwhiteUri);

  const lpUri = process.env.LP_URI ?? '//LP_1';
  lp = keyring.createFromUri(lpUri);

  // We need USDC to complete before the others.
  await setupCurrency('usdc');

  await Promise.all([
    setupCurrency('dot'),
    setupCurrency('eth'),
    setupCurrency('btc'),
  ]);
  process.exit(0);
}

runWithTimeout(main(), 2400000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
