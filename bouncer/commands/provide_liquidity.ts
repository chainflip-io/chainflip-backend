#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund liquidity of the given currency and amount
// For example: ./commands/provide_liquidity.sh btc 1.5


import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { observeEvent, getChainflipApi, runWithTimeout, handleSubstrateError } from '../shared/utils';
import { send } from '../shared/send';
import { Asset } from "@chainflip-io/cli/.";

const chain = new Map<string, string>([
	["dot", "dot"],
	["eth", "eth"],
	["btc", "btc"],
	["usdc", "eth"],
	["flip", "eth"]
]);

async function main(){
	const ccy = process.argv[2];
	const amount = process.argv[3];
	const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
	await cryptoWaitReady();

	const keyring = new Keyring({type: 'sr25519'});
	const lp_uri = process.env.SNOWWHITE_URI || '//LP_1';
	const lp = keyring.createFromUri(lp_uri);

	console.log("Requesting " + ccy + " deposit address");
	var event = observeEvent('liquidityProvider:LiquidityDepositAddressReady', chainflip, (data) => {
		return data[1][chain.get(ccy)!] != undefined;
	});
	await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress(ccy).signAndSend(lp, {nonce: -1}, handleSubstrateError(chainflip));
	var ingress_key = (await event).depositAddress.toJSON()[chain.get(ccy)!];
    var ingress_address = ingress_key;
	if(ccy == 'btc'){
		ingress_address = '';
		for(var n=2; n<ingress_key.length; n+=2){
			ingress_address += String.fromCharCode(parseInt(ingress_key.substr(n, 2), 16));
		}
	}
	console.log("Received " + ccy + " address: " + ingress_address);
	console.log("Sending " + amount + " " + ccy + " to " + ingress_address);
	event = observeEvent('liquidityProvider:AccountCredited', chainflip, (data) => {
		return data[1].toLowerCase() == ccy;
	});
	send(ccy.toUpperCase() as Asset, ingress_address, amount);
	await event;
	process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
	console.error(error);
	process.exit(-1);
});