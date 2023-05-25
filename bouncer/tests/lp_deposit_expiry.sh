#!/usr/bin/env node

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { exec } = require('child_process');
const { runWithTimeout } = require('../shared/utils');

var chainflip;

async function observe_event(eventName){
	var result;
	var waiting = true;
	let unsubscribe = await chainflip.query.system.events((events) => {
		events.forEach((record) => {
			const {event, phase} = record;
			if(event.section === eventName.split(':')[0] && event.method === eventName.split(':')[1]){
				result = event.data;
				waiting = false;
				unsubscribe();
			}
		});
	});
	while(waiting) {
		await new Promise(r => setTimeout(r, 1000));
	};
	return result;
}

async function main() {
	const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const lp_uri = process.env.LP_URI || '//LP_1';
	const lp = keyring.createFromUri(lp_uri);
	const snowwhite_uri = process.env.SNOWWHITE_URI || 'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
	const snowwhite = keyring.createFromUri(snowwhite_uri);
	chainflip = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});

	console.log("=== Testing expiry of funded LP deposit address ===")
	console.log("Setting expiry time for LP addresses to 10 blocks")
	await chainflip.tx.governance.proposeGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(10)).signAndSend(snowwhite, {nonce: -1});
	await observe_event("liquidityProvider:LpTtlSet");
	console.log("Requesting new BTC LP deposit address")
	await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress("Btc").signAndSend(lp, {nonce: -1});
	const ingress_key = (await observe_event('liquidityProvider:LiquidityDepositAddressReady'))[1].toJSON().btc;
	var ingress_address = '';
	for(var n=2; n<ingress_key.length; n+=2){
		ingress_address += String.fromCharCode(parseInt(ingress_key.substr(n, 2), 16));
	}
	exec('./commands/fund_btc.sh ' + ingress_address + ' 1', {timeout: 30000}, (err, stdout, stderr) => {
		if(stderr) process.stdout.write(stderr);
		if(err){
			console.error(err);
			process.exit(1);
		}
		if(stdout) process.stdout.write(stdout);
	});
	await observe_event('liquidityProvider:LiquidityDepositAddressExpired');
	console.log("Setting expiry time for LP addresses to 100 blocks")
	await chainflip.tx.governance.proposeGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(100)).signAndSend(snowwhite, {nonce: -1});
	await observe_event("liquidityProvider:LpTtlSet");
	console.log("=== Test complete ===")
	process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
	console.error(error);
	process.exit(-1);
});