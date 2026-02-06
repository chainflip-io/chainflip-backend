#!/usr/bin/env -S pnpm tsx
import axios from 'axios';
import { InternalAsset as Asset, Asset as RpcAsset, Chain, getInternalAsset } from '@chainflip/cli';
import bitcoin from 'bitcoinjs-lib';
import { Tapleaf } from 'bitcoinjs-lib/src/types';
import * as ecc from 'tiny-secp256k1';
import { blake2AsHex } from 'polkadot/util-crypto';
import {
  asciiStringToBytesArray,
  hexStringToBytesArray,
  sleep,
  fineAmountToAmount,
  assetDecimals,
  stateChainAssetFromAsset,
} from 'shared/utils';
import { requestNewSwap } from 'shared/perform_swap';
import { testSwap } from 'shared/swapping';
import { sendBtc } from 'shared/send_btc';
import { createLpPool } from 'shared/create_lp_pool';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { getChainflipApi } from 'shared/utils/substrate';
import { globalLogger } from 'shared/utils/logger';
import { lpApiEndpoint } from 'shared/json_rpc';
import { ChainflipIO, fullAccountFromUri, newChainflipIO } from 'shared/utils/chainflip_io';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function call(method: string, params: any, id: string) {
  return axios({
    method: 'post',
    baseURL: lpApiEndpoint,
    headers: { 'Content-Type': 'application/json' },
    data: {
      jsonrpc: '2.0',
      id,
      method,
      params,
    },
  });
}

type AmountChange = null | {
  Decrease?: number;
  Increase?: number;
};

type LimitOrderResponse = {
  base_asset: {
    chain: Chain;
    asset: RpcAsset;
  };
  quote_asset: {
    chain: Chain;
    asset: RpcAsset;
  };
  side: string;
  id: number;
  tick: number;
  sell_amount_total: number;
  collected_fees: number;
  bought_amount: number;
  sell_amount_change: AmountChange;
};

function predictBtcAddress(pubkey: string, salt: number): string {
  const saltScript = salt === 0 ? 'OP_0' : bitcoin.script.number.encode(salt).toString('hex');
  const script = bitcoin.script.fromASM(`${saltScript} OP_DROP ${pubkey} OP_CHECKSIG`);
  const scriptTree: Tapleaf = { output: script };
  const address =
    bitcoin.payments.p2tr({
      internalPubkey: Buffer.from(
        'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee',
        'hex',
      ),
      scriptTree,
      network: bitcoin.networks.regtest,
    }).address ?? '';
  return address;
}

// how to use this function:
/*
let chainflip = await getChainflipApi();
let pubkey = ((await chainflip.query.environment.polkadotVaultAccountId()).toJSON()! as string).substring(2);
let salt = ((await chainflip.query.polkadotIngressEgress.channelIdCounter()).toJSON()! as number) + 1;
let dot = predictDotAddress(pubkey, salt);
logger.info(dot); */

// eslint-disable-next-line @typescript-eslint/no-unused-vars
function predictDotAddress(pubkey: string, salt: number): string {
  const bufferSize = 16 + 32 + 2;
  const buffer = new Uint8Array(bufferSize);
  buffer.set(asciiStringToBytesArray('modlpy/utilisuba'), 0);
  buffer.set(hexStringToBytesArray(pubkey), 16);
  const littleEndianSalt = new ArrayBuffer(2);
  new DataView(littleEndianSalt).setUint16(0, salt, true);
  buffer.set(new Uint8Array(littleEndianSalt), 16 + 32);
  const result = blake2AsHex(buffer, 256);
  return result;
}

function price2tick(price: number): number {
  return Math.round(Math.log(Math.sqrt(price)) / Math.log(Math.sqrt(1.0001)));
}

// Workaround needed because legacy assets are returned as strings.
export function assetFromStateChainAsset(
  stateChainAsset:
    | {
        asset: RpcAsset;
        chain: Chain;
      }
    | string,
): Asset {
  //  DOT, BTC and Ethereum assets
  if (typeof stateChainAsset === 'string') {
    return (stateChainAsset.charAt(0) + stateChainAsset.slice(1).toLowerCase()) as Asset;
  }

  return getInternalAsset({
    asset: stateChainAsset.asset as RpcAsset,
    chain: stateChainAsset.chain as Chain,
  });
}

async function playLp<A = []>(cf: ChainflipIO<A>, asset: Asset, price: number, liquidity: number) {
  const spread = 0.01 * price;
  const liquidityFine = liquidity * 1e6;
  for (;;) {
    const offset = (price * (Math.random() - 0.5)) / 20;
    const buyTick = price2tick(price + offset + spread);
    const sellTick = price2tick(price + offset - spread);
    const result = await Promise.all([
      call(
        'lp_set_limit_order',
        [
          stateChainAssetFromAsset(asset),
          {
            chain: 'Ethereum',
            asset: 'USDC',
          },
          'buy',
          1,
          buyTick,
          '0x' + BigInt(liquidityFine).toString(16),
        ],
        `Buy ${asset}`,
      ),
      call(
        'lp_set_limit_order',
        [
          stateChainAssetFromAsset(asset),
          {
            chain: 'Ethereum',
            asset: 'USDC',
          },
          'sell',
          1,
          sellTick,
          '0x' + BigInt(liquidityFine / price).toString(16),
        ],
        `Sell ${asset}`,
      ),
    ]);
    result.forEach((r) => {
      if (r.data.error) {
        cf.error(`[${r.data.id}]: ${JSON.stringify(r.data.error)}`);
      } else {
        r.data.result.tx_details.response.forEach((update: LimitOrderResponse) => {
          if (BigInt(update.collected_fees) > BigInt(0)) {
            let ccy;
            if (update.side === 'buy') {
              ccy = assetFromStateChainAsset(update.base_asset);
            } else {
              ccy = assetFromStateChainAsset(update.quote_asset);
            }
            const fees = fineAmountToAmount(
              BigInt(update.collected_fees.toString()).toString(10),
              assetDecimals(ccy),
            );
            cf.info(`Collected ${fees} ${ccy} in fees`);
          }
          if (BigInt(update.bought_amount) > BigInt(0)) {
            let buyCcy;
            let sellCcy;
            if (update.side === 'buy') {
              buyCcy = assetFromStateChainAsset(update.base_asset);
              sellCcy = assetFromStateChainAsset(update.quote_asset);
            } else {
              buyCcy = assetFromStateChainAsset(update.quote_asset);
              sellCcy = assetFromStateChainAsset(update.base_asset);
            }
            const amount = fineAmountToAmount(
              BigInt(update.bought_amount.toString()).toString(10),
              assetDecimals(buyCcy),
            );
            cf.info(`Bought ${amount} ${buyCcy} for ${sellCcy}`);
          }
        });
      }
    });
    await sleep(12000);
  }
}

async function launchTornado<A = []>(cf: ChainflipIO<A>) {
  await using chainflip = await getChainflipApi();
  const epoch = (
    await chainflip.query.bitcoinThresholdSigner.currentKeyEpoch()
  ).toJSON()! as number;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const thresholdKeys = (await chainflip.query.bitcoinThresholdSigner.keys(epoch)).toJSON() as any;
  const pubkey = (thresholdKeys!.current as string).substring(2);
  const salt =
    ((await chainflip.query.bitcoinIngressEgress.channelIdCounter()).toJSON()! as number) + 1;
  const btcAddress = predictBtcAddress(pubkey, salt);
  // shuffle
  const assets: Asset[] = ['Eth', 'Usdc', 'Flip', 'Usdt', 'Wbtc', 'ArbEth', 'ArbUsdc', 'ArbUsdt'];
  for (let i = 0; i < 10; i++) {
    const index1 = Math.floor(Math.random() * assets.length);
    const index2 = Math.floor(Math.random() * assets.length);
    const temp = assets[index1];
    assets[index1] = assets[index2];
    assets[index2] = temp;
  }
  let swap = await requestNewSwap(cf, assets[0], 'Btc', btcAddress);
  for (let i = 0; i < assets.length - 1; i++) {
    swap = await requestNewSwap(cf, assets[i + 1], assets[i], swap.depositAddress);
  }
  await requestNewSwap(cf, 'Btc', assets[assets.length - 1], swap.depositAddress);
  await sendBtc(cf.logger, btcAddress, 0.01);
  cf.info(btcAddress);
}

const swapAmount = new Map<Asset, string>([
  ['Eth', '0.03'],
  ['Btc', '0.006'],
  ['Usdc', '30'],
  ['Usdt', '12'],
  ['Wbtc', '0.006'],
  ['Flip', '3'],
  ['ArbEth', '0.03'],
  ['ArbUsdc', '30'],
  ['ArbUsdt', '12'],
]);

async function playSwapper<A = []>(cf: ChainflipIO<A>) {
  const assets: Asset[] = [
    'Eth',
    'Btc',
    'Usdc',
    'Flip',
    'Usdt',
    'Wbtc',
    'ArbEth',
    'ArbUsdc',
    'ArbUsdt',
  ];
  for (;;) {
    const src = assets.at(Math.floor(Math.random() * assets.length))!;
    const dest = assets
      .filter((x) => x !== src)
      .at(Math.floor(Math.random() * (assets.length - 1)))!;
    /* eslint-disable @typescript-eslint/no-floating-promises */
    testSwap(cf, src, dest, undefined, undefined, undefined, undefined, swapAmount.get(src));
    await sleep(5000);
  }
}

const price = new Map<Asset, number>([
  ['Eth', 1000],
  ['Btc', 10000],
  ['Usdc', 1],
  ['Usdt', 1],
  ['Wbtc', 10000],
  ['Flip', 10],
  ['ArbEth', 1000],
  ['ArbUsdc', 1],
  ['ArbUsdt', 1],
]);

async function bananas() {
  const cf = await newChainflipIO(globalLogger, {
    account: fullAccountFromUri('//LP_1', 'LP'),
  });
  const liquidityUsdc = 10000;

  await Promise.all([
    createLpPool(cf.logger, 'Eth', price.get('Eth')!),
    createLpPool(cf.logger, 'Btc', price.get('Btc')!),
    createLpPool(cf.logger, 'Flip', price.get('Flip')!),
    createLpPool(cf.logger, 'Usdt', price.get('Usdt')!),
    createLpPool(cf.logger, 'Wbtc', price.get('Wbtc')!),
    createLpPool(cf.logger, 'ArbEth', price.get('ArbEth')!),
    createLpPool(cf.logger, 'ArbUsdc', price.get('ArbUsdc')!),
    createLpPool(cf.logger, 'ArbUsdt', price.get('ArbUsdc')!),
  ]);

  await cf.all([
    (subcf) => depositLiquidity(subcf, 'Usdc', 8 * liquidityUsdc),
    (subcf) => depositLiquidity(subcf, 'Eth', (2 * liquidityUsdc) / price.get('Eth')!),
    (subcf) => depositLiquidity(subcf, 'Btc', (2 * liquidityUsdc) / price.get('Btc')!),
    (subcf) => depositLiquidity(subcf, 'Flip', (2 * liquidityUsdc) / price.get('Flip')!),
    (subcf) => depositLiquidity(subcf, 'Usdt', (2 * liquidityUsdc) / price.get('Usdt')!),
    (subcf) => depositLiquidity(subcf, 'Wbtc', (2 * liquidityUsdc) / price.get('Wbtc')!),
    (subcf) => depositLiquidity(subcf, 'ArbEth', (2 * liquidityUsdc) / price.get('ArbEth')!),
    (subcf) => depositLiquidity(subcf, 'ArbUsdc', (2 * liquidityUsdc) / price.get('ArbUsdc')!),
    (subcf) => depositLiquidity(subcf, 'ArbUsdt', (2 * liquidityUsdc) / price.get('ArbUsdt')!),
  ]);

  await cf.all([
    (subcf) =>
      playLp(
        subcf,
        'Eth',
        price.get('Eth')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Eth')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'Btc',
        price.get('Btc')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Btc')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'Flip',
        price.get('Flip')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Flip')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'Usdt',
        price.get('Usdt')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Usdt')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'Wbtc',
        price.get('Wbtc')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Wbtc')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'ArbEth',
        price.get('ArbEth')! * 10 ** (assetDecimals('Usdc') - assetDecimals('ArbEth')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'ArbUsdc',
        price.get('ArbUsdc')! * 10 ** (assetDecimals('Usdc') - assetDecimals('ArbUsdc')),
        liquidityUsdc,
      ),
    (subcf) =>
      playLp(
        subcf,
        'ArbUsdt',
        price.get('ArbUsdt')! * 10 ** (assetDecimals('Usdc') - assetDecimals('ArbUsdt')),
        liquidityUsdc,
      ),
    (subcf) => playSwapper(subcf),
    (subcf) => launchTornado(subcf),
  ]);
}

bitcoin.initEccLib(ecc);
await bananas();
process.exit(0);
