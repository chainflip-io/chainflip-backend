#!/usr/bin/env -S pnpm tsx
import axios from 'axios';
import { Asset } from '@chainflip-io/cli/.';
import bitcoin from 'bitcoinjs-lib';
import { Tapleaf } from 'bitcoinjs-lib/src/types';
import { blake2AsHex } from '@polkadot/util-crypto';
import {
  asciiStringToBytesArray,
  getChainflipApi,
  hexStringToBytesArray,
  sleep,
} from '../shared/utils';
import { requestNewSwap } from '../shared/perform_swap';
import { testSwap } from '../shared/swapping';
import { sendBtc } from '../shared/send_btc';
import { createLpPool } from '../shared/create_lp_pool';
import { provideLiquidity } from '../shared/provide_liquidity';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function call(method: string, data: any, id: string) {
  return axios({
    method: 'post',
    baseURL: 'http://localhost:10589',
    headers: { 'Content-Type': 'application/json' },
    data: {
      jsonrpc: '2.0',
      id,
      method,
      params: data,
    },
  });
}

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

async function playLp(asset: string, price: number, liquidity: number) {
  let offset = 0;
  const spread = 0.01 * price;
  const liquidityFine = liquidity * 1e6;
  for (;;) {
    const buyTick = price2tick(price + offset + spread);
    const sellTick = price2tick(price + offset - spread);
    const newOffset = (price * (Math.random() - 0.5)) / 20;
    const newBuyTick = price2tick(price + newOffset + spread);
    const newSellTick = price2tick(price + newOffset - spread);
    const result = await Promise.all([
      call(
        'lp_burnLimitOrder',
        [asset, 'Buy', buyTick, liquidityFine.toString(16)],
        `Burn Buy ${asset}`,
      ),
      call(
        'lp_burnLimitOrder',
        [asset, 'Sell', sellTick, (liquidityFine / price).toString(16)],
        `Burn Sell ${asset}`,
      ),
      call(
        'lp_mintLimitOrder',
        [asset, 'Buy', newBuyTick, liquidityFine.toString(16)],
        `Mint Buy ${asset}`,
      ),
      call(
        'lp_mintLimitOrder',
        [asset, 'Sell', newSellTick, (liquidityFine / price).toString(16)],
        `Mint Sell ${asset}`,
      ),
    ]);
    result.forEach((r) => {
      if (r.data.error) {
        console.log(`Error [${r.data.id}]: ${JSON.stringify(r.data.error)}`);
      } else if (r.data.result.swapped_liquidity > 0) {
        console.log(`Swapped ${r.data.result.swapped_liquidity} ${r.data.id}`);
      }
    });
    offset = newOffset;
    await sleep(5000);
  }
}

async function launchTornado() {
  const chainflip = await getChainflipApi();
  const epoch = (await chainflip.query.bitcoinVault.currentVaultEpochAndState()).toJSON()!
    .epochIndex as number;
  const pubkey = (
    (await chainflip.query.bitcoinVault.vaults(epoch)).toJSON()!.publicKey.current as string
  ).substring(2);
  const salt =
    ((await chainflip.query.bitcoinIngressEgress.channelIdCounter()).toJSON()! as number) + 1;
  const btcAddress = predictBtcAddress(pubkey, salt);
  // shuffle
  const assets: Asset[] = ['ETH', 'USDC', 'FLIP', 'DOT'];
  for (let i = 0; i < 10; i++) {
    const index1 = Math.floor(Math.random() * assets.length);
    const index2 = Math.floor(Math.random() * assets.length);
    const temp = assets[index1];
    assets[index1] = assets[index2];
    assets[index2] = temp;
  }
  let swap = await requestNewSwap(assets[0], 'BTC', btcAddress);
  for (let i = 0; i < assets.length - 1; i++) {
    swap = await requestNewSwap(assets[i + 1], assets[i], swap.depositAddress);
  }
  await requestNewSwap('BTC', assets[assets.length - 1], swap.depositAddress);
  await sendBtc(btcAddress, 1);
  console.log(btcAddress);
}

async function playSwapper() {
  const assets: Asset[] = ['ETH', 'BTC', 'USDC', 'FLIP', 'DOT'];
  for (;;) {
    const src = assets.at(Math.floor(Math.random() * assets.length))!;
    const dest = assets
      .filter((x) => x !== src)
      .at(Math.floor(Math.random() * (assets.length - 1)))!;
    testSwap(src, dest);
    await sleep(5000);
  }
}

const price = new Map<Asset, number>([
  ['DOT', 10],
  ['ETH', 1000],
  ['BTC', 10000],
  ['USDC', 1],
  ['FLIP', 10],
]);

async function bananas() {
  const liquidityUsdc = 100000;

  await Promise.all([
    createLpPool('ETH', price.get('ETH')!),
    createLpPool('DOT', price.get('DOT')!),
    createLpPool('BTC', price.get('BTC')!),
    createLpPool('FLIP', price.get('FLIP')!),
  ]);

  await Promise.all([
    provideLiquidity('USDC', 8 * liquidityUsdc),
    provideLiquidity('ETH', (2 * liquidityUsdc) / price.get('ETH')!),
    provideLiquidity('DOT', (2 * liquidityUsdc) / price.get('DOT')!),
    provideLiquidity('BTC', (2 * liquidityUsdc) / price.get('BTC')!),
    provideLiquidity('FLIP', (2 * liquidityUsdc) / price.get('FLIP')!),
  ]);
  await Promise.all([
    playLp('Eth', price.get('ETH')! * 10 ** (6 - 18), liquidityUsdc),
    playLp('Btc', price.get('BTC')! * 10 ** (6 - 8), liquidityUsdc),
    playLp('Dot', price.get('DOT')! * 10 ** (6 - 8), liquidityUsdc),
    playLp('Flip', price.get('FLIP')! * 10 ** (6 - 8), liquidityUsdc),
    playSwapper(),
    launchTornado(),
  ]);
}

await bananas();
process.exit(0);
