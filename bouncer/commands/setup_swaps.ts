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
import { submitGovernanceExtrinsic } from '../shared/cf_governance';
import {
  runWithTimeout,
  sleep,
  getAddress,
  hexStringToBytesArray,
  asciiStringToBytesArray,
  assetToDecimals,
  handleSubstrateError,
} from '../shared/utils';
import { Asset } from '@chainflip-io/cli/.';

const deposits = new Map<Asset, number>([
  ['DOT', 10000],
  ['ETH', 100],
  ['BTC', 10],
  ['USDC', 1000000],
  ['FLIP', 10000],
]);

const values = new Map<Asset, number>([
  ['DOT', 10],
  ['ETH', 1000],
  ['BTC', 10000],
  ['USDC', 1],
  ['FLIP', 10],
]);

const chain = new Map<Asset, string>([
  ['DOT', 'dot'],
  ['ETH', 'eth'],
  ['BTC', 'btc'],
  ['USDC', 'eth'],
  ['FLIP', 'flip'],
]);

const cfNodeEndpoint = process.env.CF_NODE_ENDPOINT ?? 'ws://127.0.0.1:9944';
let chainflip: ApiPromise;
let keyring: Keyring;
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

export async function setupEmergencyWithdrawalAddress(address: any): Promise<void> {
  console.log('Registering Emergency Withdrawal Address. Address:' + address);
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .registerEmergencyWithdrawalAddress(address)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
}

async function setupCurrency(ccy: Asset): Promise<void> {
  console.log('Requesting ' + ccy + ' deposit address');
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy.toLowerCase())
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const checkCcy = (data: any): boolean => {
    const result = data[1].toJSON()[chain.get(ccy)!];
    return result !== null && result !== undefined;
  };

  const ingressKey = (
    await observeEvent('liquidityProvider:LiquidityDepositAddressReady', checkCcy)
  )[1].toJSON()[chain.get(ccy)!] as string;
  let ingressAddress = ingressKey;
  if (ccy === 'BTC') {
    ingressAddress = '';
    for (let n = 2; n < ingressKey.length; n += 2) {
      ingressAddress += String.fromCharCode(parseInt(ingressKey.substr(n, 2), 16));
    }
  }
  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  exec(
    'pnpm tsx ./commands/send_' +
      ccy.toLowerCase() +
      '.ts' +
      ' ' +
      ingressAddress +
      ' ' +
      deposits.get(ccy),
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
  const checkDeposit = (data: any): boolean => data.asset.toJSON().toUpperCase() === ccy;

  await observeEvent('liquidityProvider:AccountCredited', checkDeposit);
  if (ccy === 'USDC') {
    return;
  }
  const price = BigInt(
    Math.round(
      Math.sqrt(
        values.get(ccy)! / 10 ** (assetToDecimals.get(ccy)! - assetToDecimals.get('USDC')!),
      ) *
        2 ** 96,
    ),
  );
  console.log('Setting up ' + ccy + ' pool');

  await submitGovernanceExtrinsic(chainflip.tx.liquidityPools.newPool(ccy, 100, price));

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const checkPool = (data: any): boolean => data.unstableAsset.toJSON().toUpperCase() === ccy;

  await observeEvent('liquidityPools:NewPoolCreated', checkPool);
  const priceTick = Math.round(
    Math.log(
      Math.sqrt(
        values.get(ccy)! / 10 ** (assetToDecimals.get(ccy)! - assetToDecimals.get('USDC')!),
      ),
    ) / Math.log(Math.sqrt(1.0001)),
  );
  const buyPosition = deposits.get(ccy)! * values.get(ccy)! * 1000000;
  console.log(
    'Placing Buy Limit order for ' +
      deposits.get(ccy)! +
      ' ' +
      ccy +
      ' at ' +
      values.get(ccy)! +
      ' USDC.',
  );
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .collectAndMintLimitOrder(ccy.toLowerCase(), 'Buy', priceTick, buyPosition)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  console.log(
    'Placing Sell Limit order for ' +
      deposits.get(ccy)! +
      ' ' +
      ccy +
      ' at ' +
      values.get(ccy)! +
      ' USDC.',
  );
  const sellPosition = BigInt(deposits.get(ccy)! * 10 ** assetToDecimals.get(ccy)!);
  await mutex.runExclusive(async () => {
    await chainflip.tx.liquidityPools
      .collectAndMintLimitOrder(ccy.toLowerCase(), 'Sell', priceTick, sellPosition)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
}

async function main(): Promise<void> {
  chainflip = await ApiPromise.create({
    provider: new WsProvider(cfNodeEndpoint),
    noInitWarn: true,
    types: {
      EncodedAddress: {
        _enum: {
          Eth: '[u8; 20]',
          Dot: '[u8; 32]',
          Btc: '[u8; 34]',
        },
      },
    },
  });
  await cryptoWaitReady();

  keyring = new Keyring({ type: 'sr25519' });

  const lpUri = process.env.LP_URI ?? '//LP_1';
  lp = keyring.createFromUri(lpUri);

  // Register Emergency withdrawal address for all chains
  const encodedEthAddr = chainflip.createType('EncodedAddress', {
    Eth: hexStringToBytesArray(await getAddress('ETH', 'LP_1')),
  });
  const encodedDotAddr = chainflip.createType('EncodedAddress', { Dot: lp.publicKey });
  const encodedBtcAddr = chainflip.createType('EncodedAddress', {
    Btc: asciiStringToBytesArray(await getAddress('BTC', 'LP_1')),
  });

  await setupEmergencyWithdrawalAddress(encodedEthAddr);
  await setupEmergencyWithdrawalAddress(encodedDotAddr);
  await setupEmergencyWithdrawalAddress(encodedBtcAddr);

  // We need USDC to complete before the others.
  await setupCurrency('USDC');

  await Promise.all([setupCurrency('DOT'), setupCurrency('ETH'), setupCurrency('BTC')]);
  process.exit(0);
}

runWithTimeout(main(), 2400000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
