#!/usr/bin/env -S pnpm tsx
import axios from 'axios';
import { InternalAsset as Asset, Chain, getInternalAsset } from '@chainflip/cli';
import bitcoin from 'bitcoinjs-lib';
import { Tapleaf } from 'bitcoinjs-lib/src/types';
import { blake2AsHex } from '@polkadot/util-crypto';
import * as ecc from 'tiny-secp256k1';
import {
  asciiStringToBytesArray,
  getChainflipApi,
  hexStringToBytesArray,
  sleep,
  fineAmountToAmount,
  assetDecimals,
  chainFromAsset,
  stateChainAssetFromAsset,
} from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { testSwap } from '../shared/swapping';
import { sendBtc } from '../shared/send_btc';
import { createLpPool } from '../shared/create_lp_pool';
import { provideLiquidity } from '../shared/provide_liquidity';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function call(method: string, params: any, id: string) {
  return axios({
    method: 'post',
    baseURL: 'http://127.0.0.1:10589',
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
    chain: string;
    asset: Asset;
  };
  quote_asset: {
    chain: string;
    asset: Asset;
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
console.log(dot); */

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
        asset: Asset | string;
        chain: Chain | string;
      }
    | string,
): Asset {
  //  DOT, BTC and Ethereum assets
  if (typeof stateChainAsset === 'string') {
    return (stateChainAsset.charAt(0) + stateChainAsset.slice(1).toLowerCase()) as Asset;
  }

  return getInternalAsset(stateChainAsset);
}

async function playLp(asset: Asset, price: number, liquidity: number) {
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
          {
            chain: chainFromAsset(asset),
            asset: stateChainAssetFromAsset(asset),
          },
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
          {
            chain: chainFromAsset(asset),
            asset: stateChainAssetFromAsset(asset),
          },
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
        console.log(`Error [${r.data.id}]: ${JSON.stringify(r.data.error)}`);
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
            console.log(`Collected ${fees} ${ccy} in fees`);
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
            console.log(`Bought ${amount} ${buyCcy} for ${sellCcy}`);
          }
        });
      }
    });
    await sleep(12000);
  }
}

async function launchTornado() {
  await using chainflip = await getChainflipApi();
  const epoch = (
    await chainflip.query.bitcoinThresholdSigner.currentKeyEpoch()
  ).toJSON()! as number;
  const pubkey = (
    (await chainflip.query.bitcoinThresholdSigner.keys(epoch)).toJSON()!.current as string
  ).substring(2);
  const salt =
    ((await chainflip.query.bitcoinIngressEgress.channelIdCounter()).toJSON()! as number) + 1;
  const btcAddress = predictBtcAddress(pubkey, salt);
  // shuffle
  const assets: Asset[] = ['Eth', 'Usdc', 'Flip', 'Dot', 'Usdt', 'ArbEth', 'ArbUsdc'];
  for (let i = 0; i < 10; i++) {
    const index1 = Math.floor(Math.random() * assets.length);
    const index2 = Math.floor(Math.random() * assets.length);
    const temp = assets[index1];
    assets[index1] = assets[index2];
    assets[index2] = temp;
  }
  let swap = await requestNewSwap(assets[0], 'Btc', btcAddress);
  for (let i = 0; i < assets.length - 1; i++) {
    swap = await requestNewSwap(assets[i + 1], assets[i], swap.depositAddress);
  }
  await requestNewSwap('Btc', assets[assets.length - 1], swap.depositAddress);
  await sendBtc(btcAddress, 0.01);
  console.log(btcAddress);
}

const swapAmount = new Map<Asset, string>([
  ['Dot', '3'],
  ['Eth', '0.03'],
  ['Btc', '0.006'],
  ['Usdc', '30'],
  ['Usdt', '12'],
  ['Flip', '3'],
  ['ArbEth', '0.03'],
  ['ArbUsdc', '30'],
]);

async function playSwapper() {
  const assets: Asset[] = ['Eth', 'Btc', 'Usdc', 'Flip', 'Dot', 'Usdt', 'ArbEth', 'ArbUsdc'];
  for (;;) {
    const src = assets.at(Math.floor(Math.random() * assets.length))!;
    const dest = assets
      .filter((x) => x !== src)
      .at(Math.floor(Math.random() * (assets.length - 1)))!;
    testSwap(src, dest, undefined, undefined, undefined, swapAmount.get(src));
    await sleep(5000);
  }
}

const price = new Map<Asset, number>([
  ['Dot', 10],
  ['Eth', 1000],
  ['Btc', 10000],
  ['Usdc', 1],
  ['Usdt', 1],
  ['Flip', 10],
  ['ArbEth', 1000],
  ['ArbUsdc', 1],
]);

async function bananas() {
  const liquidityUsdc = 10000;

  await Promise.all([
    createLpPool('Eth', price.get('Eth')!),
    createLpPool('Dot', price.get('Dot')!),
    createLpPool('Btc', price.get('Btc')!),
    createLpPool('Flip', price.get('Flip')!),
    createLpPool('Usdt', price.get('Usdt')!),
    createLpPool('ArbEth', price.get('ArbEth')!),
    createLpPool('ArbUsdc', price.get('ArbUsdc')!),
  ]);

  await Promise.all([
    provideLiquidity('Usdc', 8 * liquidityUsdc),
    provideLiquidity('Eth', (2 * liquidityUsdc) / price.get('Eth')!),
    provideLiquidity('Dot', (2 * liquidityUsdc) / price.get('Dot')!),
    provideLiquidity('Btc', (2 * liquidityUsdc) / price.get('Btc')!),
    provideLiquidity('Flip', (2 * liquidityUsdc) / price.get('Flip')!),
    provideLiquidity('Usdt', (2 * liquidityUsdc) / price.get('Usdt')!),
    provideLiquidity('ArbEth', (2 * liquidityUsdc) / price.get('ArbEth')!),
    provideLiquidity('ArbUsdc', (2 * liquidityUsdc) / price.get('ArbUsdc')!),
  ]);

  await Promise.all([
    playLp(
      'Eth',
      price.get('Eth')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Eth')),
      liquidityUsdc,
    ),
    playLp(
      'Btc',
      price.get('Btc')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Btc')),
      liquidityUsdc,
    ),
    playLp(
      'Dot',
      price.get('Dot')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Dot')),
      liquidityUsdc,
    ),
    playLp(
      'Flip',
      price.get('Flip')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Flip')),
      liquidityUsdc,
    ),
    playLp(
      'Usdt',
      price.get('Usdt')! * 10 ** (assetDecimals('Usdc') - assetDecimals('Usdt')),
      liquidityUsdc,
    ),
    playLp(
      'ArbEth',
      price.get('ArbEth')! * 10 ** (assetDecimals('Usdc') - assetDecimals('ArbEth')),
      liquidityUsdc,
    ),
    playLp(
      'ArbUsdc',
      price.get('ArbUsdc')! * 10 ** (assetDecimals('Usdc') - assetDecimals('ArbUsdc')),
      liquidityUsdc,
    ),
    playSwapper(),
    launchTornado(),
  ]);
}

bitcoin.initEccLib(ecc);
await bananas();
process.exit(0);
