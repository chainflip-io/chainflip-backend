#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes no arguments.
// It will perform the initial polkadot vault setup procedure described here
// https://www.notion.so/chainflip/Polkadot-Vault-Initialisation-Steps-36d6ab1a24ed4343b91f58deed547559
// For example: ./commands/setup_polkadot_vault.sh

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { exec } = require('child_process');
const { runWithTimeout } = require('../shared/utils');
const { Mutex } = require('async-mutex');

const deposits = {
	dot: 10000,
	eth: 100,
	btc: 10,
	usdc: 1000000
};

const values = {
	dot: 10,
	eth: 1000,
	btc: 10000
};

const decimals = {
	dot: 10,
	eth: 18,
	btc: 8,
	usdc: 6
};

const chain = {
	dot: 'dot',
	btc: 'btc',
	eth: 'eth',
	usdc: 'eth'
};

const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
var chainflip, keyring, snowwhite, lp;
const mutex = new Mutex();

async function observe_event(eventName, dataCheck){
	var result;
	var waiting = true;
	let unsubscribe = await chainflip.query.system.events((events) => {
		events.forEach((record) => {
			const {event, phase} = record;
			if(event.section === eventName.split(':')[0] && event.method === eventName.split(':')[1]){
				if(dataCheck(event.data)){
					result = event.data;
					waiting = false;
					unsubscribe();
				}
			}
		});
	});
	while(waiting) {
		await new Promise(r => setTimeout(r, 1000));
	};
	return result;
}

async function setup_currency(ccy){
	console.log("Requesting " + ccy + " deposit address");
	await mutex.runExclusive(async () => {
		await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress(ccy).signAndSend(lp, {nonce: -1});
	});
	var check_ccy = (data) => {
		return data[1].toJSON()[chain[ccy]] != null;
	}
	const ingress_key = (await observe_event('liquidityProvider:LiquidityDepositAddressReady', check_ccy))[1].toJSON()[chain[ccy]];
	var ingress_address = ingress_key;
	if(ccy == 'btc'){
		ingress_address = '';
		for(var n=2; n<ingress_key.length; n+=2){
			ingress_address += String.fromCharCode(parseInt(ingress_key.substr(n, 2), 16));
		}
	}
	console.log("Received " + ccy + " address: " + ingress_address);
	exec('./commands/fund_' + ccy + '.sh ' + ingress_address + ' ' + deposits[ccy], {timeout: 30000}, (err, stdout, stderr) => {
		if(stderr) process.stdout.write(stderr);
			if(err){
				console.error(err);
				process.exit(1);
			}
			if(stdout) process.stdout.write(stdout);
		});
	const check_deposit = (data) => {
		return data.asset.toJSON().toLowerCase() == ccy;
	}
	await observe_event('liquidityProvider:AccountCredited', check_deposit);
	if(ccy == 'usdc'){
		return;
	}
	const price = BigInt(Math.round(Math.sqrt(values[ccy]/Math.pow(10, decimals[ccy]-decimals.usdc))*Math.pow(2,96)));
	console.log("Setting up " + ccy + " pool");
	await mutex.runExclusive(async () => {
		await chainflip.tx.governance.proposeGovernanceExtrinsic(chainflip.tx.liquidityPools.newPool(ccy, 100, price)).signAndSend(snowwhite, {nonce: -1});
	});
	const check_pool = (data) => {
		return data.unstableAsset.toJSON().toLowerCase() == ccy;
	}
	await observe_event('liquidityPools:NewPoolCreated', check_pool);
	const price_tick = Math.round(Math.log(Math.sqrt(values[ccy]/Math.pow(10, decimals[ccy]-decimals.usdc)))/Math.log(Math.sqrt(1.0001)));
	const buy_position = deposits[ccy]*values[ccy]*1000000.;
	console.log("Placing Buy Limit order for " + deposits[ccy] + " " + ccy + " at " + values[ccy] + " USDC.");
	await mutex.runExclusive(async () => {
		await chainflip.tx.liquidityPools.collectAndMintLimitOrder(ccy, "Buy", price_tick, buy_position).signAndSend(lp, {nonce: -1}, ({ status, events, dispatchError }) => {
		if(dispatchError){
			if(dispatchError.isModule){
				const decoded = chainflip.registry.findMetaError(dispatchError.asModule);
				const { docs, name, section } = decoded;
				console.log(`Placing Buy Limit order for ${ccy} failed: ${section}.${name}: ${docs.join(' ')}`);
			} else {
				console.log(`Placing Buy Limit order for ${ccy} failed: Error: ` + dispatchError.toString());
			}
			process.exit(-1);
		}
		if(status.isInBlock || status.isFinalized){
			waiting = false;
		}});
	});
	console.log("Placing Sell Limit order for " + deposits[ccy] + " " + ccy + " at " + values[ccy] + " USDC.");
	const sell_position = BigInt(deposits[ccy]*Math.pow(10,decimals[ccy]));
	await mutex.runExclusive(async () => {
		await chainflip.tx.liquidityPools.collectAndMintLimitOrder(ccy, "Sell", price_tick, sell_position).signAndSend(lp, {nonce: -1}, ({ status, events, dispatchError }) => {
		if(dispatchError){
			if(dispatchError.isModule){
				const decoded = chainflip.registry.findMetaError(dispatchError.asModule);
				const { docs, name, section } = decoded;
				console.log(`Placing Sell Limit order for ${ccy} failed:${section}.${name}: ${docs.join(' ')}`);
			} else {
				console.log(`Placing Sell Limit order for ${ccy} failed: Error: ` + dispatchError.toString());
			}
			process.exit(-1);
		}
		if(status.isInBlock || status.isFinalized){
			waiting = false;
		}});
	});
}

async function main() {
	chainflip = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});
	await cryptoWaitReady();

	keyring = new Keyring({type: 'sr25519'});
	const snowwhite_uri = process.env.SNOWWHITE_URI || 'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
	snowwhite = keyring.createFromUri(snowwhite_uri);

	const lp_uri = process.env.LP_URI || '//LP_1';
	lp = keyring.createFromUri(lp_uri);

	var waiting = true;

	await setup_currency('usdc');
	const dot_promise = setup_currency('dot');
	const eth_promise = setup_currency('eth');
	const btc_promise = setup_currency('btc');
	await dot_promise;
	await eth_promise;
	await btc_promise;
	process.exit(0);
}

runWithTimeout(main(), 2400000).catch((error) => {
	console.error(error);
	process.exit(-1);
});