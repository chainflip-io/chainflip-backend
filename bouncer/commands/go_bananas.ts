#!/usr/bin/env -S pnpm tsx
import axios from "axios";
import { asciiStringToBytesArray, getChainflipApi, hexStringToBytesArray, sleep } from "../shared/utils";
import { requestNewSwap } from "../shared/perform_swap";
import { Asset } from "@chainflip-io/cli/.";
import { testSwap } from "../shared/swapping";
import bitcoin from 'bitcoinjs-lib';
import { Tapleaf, Taptree } from "bitcoinjs-lib/src/types";
import { sendBtc } from "../shared/send_btc";
import { createLpPool } from "../shared/create_lp_pool";
import { provideLiquidity } from "../shared/provide_liquidity";
import { blake2AsHex } from "@polkadot/util-crypto";

async function call(method: string, data: any, id: string){
    return axios({
        method: 'post',
        baseURL: 'http://localhost:10589',
        headers: {'Content-Type': 'application/json'},
        data: {
            jsonrpc: "2.0",
            id: id,
            method: method,
            params: data
        }
    });
}

function predictBtcAddress(pubkey: string, salt: number): string {
    let saltScript = salt == 0 ? 'OP_0' : bitcoin.script.number.encode(salt).toString('hex');
    const script = bitcoin.script.fromASM(`${saltScript} OP_DROP ${pubkey} OP_CHECKSIG`);
    const scriptTree: Tapleaf = {output: script};
    const address = bitcoin.payments.p2tr({internalPubkey: Buffer.from("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "hex"), scriptTree, network: bitcoin.networks.regtest}).address ?? '';
    return address;
}

function predictDotAddress(pubkey: string, salt: number): string {
    const buffer_size = 16 + 32 + 2;
    let buffer = new Uint8Array(buffer_size);
    buffer.set(asciiStringToBytesArray("modlpy/utilisuba"), 0);
    buffer.set(hexStringToBytesArray(pubkey), 16);
    const le_salt = new ArrayBuffer(2);
    new DataView(le_salt).setUint16(0, salt, true);
    buffer.set(new Uint8Array(le_salt), 16 + 32);
    let result = blake2AsHex(buffer, 256);
    return result;
}

function price2tick(price: number): number {
    return Math.round(Math.log(Math.sqrt(price))/Math.log(Math.sqrt(1.0001)));
}

async function playLp(asset: string, price: number){
    let offset = 0;
    const spread = 0.01*price;
    const liquidity = 1000000*1e6;
    while(true){
        const buy_tick  = price2tick(price+offset+spread);
        const sell_tick = price2tick(price+offset-spread);
        let new_offset = price*(Math.random() - 0.5)/20.;
        const new_buy_tick  = price2tick(price+new_offset+spread);
        const new_sell_tick = price2tick(price+new_offset-spread);
        const result = await Promise.all([
            call('lp_burnLimitOrder', [asset, 'Buy', buy_tick, liquidity.toString(16)], `Burn Buy ${asset}`),
            call('lp_burnLimitOrder', [asset, 'Sell', sell_tick, (liquidity/price).toString(16)], `Burn Sell ${asset}`),
            call('lp_mintLimitOrder', [asset, 'Buy', new_buy_tick, liquidity.toString(16)], `Mint Buy ${asset}`),
            call('lp_mintLimitOrder', [asset, 'Sell', new_sell_tick, (liquidity/price).toString(16)], `Mint Sell ${asset}`),
        ]);
        result.forEach((r) => {
            if(r.data.error){
                console.log(`Error [${r.data.id}]: ${JSON.stringify(r.data.error)}`);
            } else {
                if(r.data.result.swapped_liquidity > 0){
                    console.log(`Swapped ${r.data.result.swapped_liquidity} ${r.data.id}`);
                }
            }
        });
        offset = new_offset;
        await sleep(5000);
    }
}

async function launchTornado(){
    let chainflip = await getChainflipApi();
    let epoch = (await chainflip.query.bitcoinVault.currentVaultEpochAndState()).toJSON()!.epochIndex as number;
    let pubkey = ((await chainflip.query.bitcoinVault.vaults(epoch)).toJSON()!.publicKey.current as string).substring(2);
    let salt = ((await chainflip.query.bitcoinIngressEgress.channelIdCounter()).toJSON()! as number) + 1;
    let btcAddress = predictBtcAddress(pubkey, salt);
    // shuffle
    let assets : Array<Asset> = ["ETH", "USDC", "FLIP", "DOT"];
    for(let i = 0; i<10; i++){
        let index1 = Math.floor(Math.random()*assets.length);
        let index2 = Math.floor(Math.random()*assets.length);
        let temp = assets[index1];
        assets[index1] = assets[index2];
        assets[index2] = temp;
    }
    let swap = await requestNewSwap(assets[0], "BTC", btcAddress);
    for(let i = 0; i<assets.length - 1; i++){
        swap = await requestNewSwap(assets[i+1], assets[i], swap.depositAddress);
    }
    await requestNewSwap("BTC", assets[assets.length-1], swap.depositAddress);
    await sendBtc(btcAddress, 1);
    console.log(btcAddress);
}

async function playSwapper(){
    let assets: Array<Asset> = ["ETH", "BTC", "USDC", "FLIP", "DOT"];
    while(true){
        let src = assets.at(Math.floor(Math.random()*assets.length))!;
        let dest = assets.filter((x) => x != src).at(Math.floor(Math.random()*(assets.length-1)))!;
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

const deposits = new Map<Asset, number>([
    ['DOT', 10000000/price.get('DOT')!],
    ['ETH', 10000000/price.get('ETH')!],
    ['BTC', 10000000/price.get('BTC')!],
    ['USDC', 50000000],
    ['FLIP', 10000000/price.get('FLIP')!],
]);

async function bananas(){
    await Promise.all([
        createLpPool('ETH', price.get('ETH')!),
        createLpPool('DOT', price.get('DOT')!),
        createLpPool('BTC', price.get('BTC')!),
        createLpPool('FLIP', price.get('FLIP')!),
    ]);

    await Promise.all([
        provideLiquidity('USDC', 2*deposits.get('USDC')!),
        provideLiquidity('ETH', 2*deposits.get('ETH')!),
        provideLiquidity('DOT', 2*deposits.get('DOT')!),
        provideLiquidity('BTC', 2*deposits.get('BTC')!),
        provideLiquidity('FLIP', 2*deposits.get('FLIP')!),
    ]);
    await Promise.all([
        playLp('Eth', price.get('ETH')!*Math.pow(10, 6-18)),
        playLp('Btc', price.get('BTC')!*Math.pow(10, 6-8)),
        playLp('Dot', price.get('DOT')!*Math.pow(10, 6-8)),
        playLp('Flip', price.get('FLIP')!*Math.pow(10, 6-8)),
        playSwapper(),
        launchTornado(),
    ]);
}

let chainflip = await getChainflipApi();
let pubkey = ((await chainflip.query.environment.polkadotVaultAccountId()).toJSON()! as string).substring(2);
let salt = ((await chainflip.query.polkadotIngressEgress.channelIdCounter()).toJSON()! as number) + 1;
let dot = predictDotAddress(pubkey, salt);
console.log(dot);
//await bananas();
process.exit(0);
