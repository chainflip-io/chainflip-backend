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
	chainflip = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});

	await chainflip.tx.liquidityProvider.requestLiquidityDepositAddress("Eth").signAndSend(lp);
	const eth_ingress_key = (await observe_event('liquidityProvider:LiquidityDepositAddressReady'))[1].toJSON().eth;
	console.log("ETH ingress address: " + eth_ingress_key);
	await new Promise(r => setTimeout(r, 8000)); //sleep for 8 seconds to give the engine a chance to start witnessing
	exec('./commands/fund_eth.sh ' + eth_ingress_key + ' 10', {timeout: 10000}, (err, stdout, stderr) => {
			if(stderr) process.stdout.write(stderr);
			if(err){
				console.error(err);
				process.exit(1);
			}
			if(stdout) process.stdout.write(stdout);
		});
	await observe_event('liquidityProvider:AccountCredited');
	exec('./commands/fund_eth.sh ' + eth_ingress_key + ' 10', {timeout: 10000}, (err, stdout, stderr) => {
			if(stderr) process.stdout.write(stderr);
			if(err){
				console.error(err);
				process.exit(1);
			}
			if(stdout) process.stdout.write(stdout);
		});
}

runWithTimeout(main(), 120000).catch((error) => {
	console.error(error);
	process.exit(-1);
});