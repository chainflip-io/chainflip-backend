#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the polkadot address provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted in DOT.
//
// For example: ./commands/fund_dot.sh 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB 1.2
// will send 1.2 DOT to account 12QTpTMELPfdz2xr9AeeavstY8uMcpUqeKWDWiwarskk4hSB

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const polkadot_endpoint = process.env.POLKADOT_ENDPOINT || 'ws://127.0.0.1:9945';
	const polkadot_address = process.argv[2];
	const dot_amount = process.argv[3].trim();
	var planck_amount;
	if(dot_amount.indexOf('.') == -1){
		planck_amount = dot_amount + "0000000000";
	} else {
		const amount_parts = dot_amount.split('.');
		planck_amount = amount_parts[0] + amount_parts[1].padEnd(10,'0').substr(0, 10);
	}
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const alice = keyring.createFromUri('//Alice');
	const polkadot = await ApiPromise.create({provider: new WsProvider(polkadot_endpoint), noInitWarn: true});

	console.log("Transferring " + dot_amount + " DOT to " + polkadot_address);
	await polkadot.tx.balances.transfer(polkadot_address, parseInt(planck_amount)).signAndSend(alice, {nonce: -1}, ({ status, events, dispatchError }) => {
		if(dispatchError){
			if(dispatchError.isModule){
				const decoded = polkadot.registry.findMetaError(dispatchError.asModule);
				const { docs, name, section } = decoded;
				console.log(`${section}.${name}: ${docs.join(' ')}`);
			} else {
				console.log("Error: " + dispatchError.toString());
			}
			process.exit(-1);
		}
		if(status.isInBlock || status.isFinalized){
			process.exit(0);
		}
	});
}

runWithTimeout(main(), 20000).catch((error) => {
	console.error(error);
	process.exit(-1);
});