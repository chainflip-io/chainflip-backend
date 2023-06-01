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
const axios = require('axios');

async function main() {
	const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	const polkadot_endpoint = process.env.POLKADOT_ENDPOINT || 'ws://127.0.0.1:9945';
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const snowwhite_uri = process.env.SNOWWHITE_URI || 'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
	const snowwhite = keyring.createFromUri(snowwhite_uri);
	const alice_uri = process.env.POLKADOT_ALICE_URI || "//Alice";
	const alice = keyring.createFromUri(alice_uri);
	const chainflip = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});
	const polkadot = await ApiPromise.create({provider: new WsProvider(polkadot_endpoint), noInitWarn: true});

	console.log("=== Performing initial Vault setup ===");

	// Step 1
	console.log("Forcing rotation");
	await chainflip.tx.governance.proposeGovernanceExtrinsic(chainflip.tx.validator.forceRotation()).signAndSend(snowwhite);

	// Step 2
	console.log("Waiting for new keys");
	var dotKey;
	var btcKey;
	var waitingForDotKey = true;
	var waitingForBtcKey = true;
	let unsubscribe = await chainflip.query.system.events((events) => {
		events.forEach((record) => {
			const {event, phase} = record;
			if(event.section === "polkadotVault" && event.method === "AwaitingGovernanceActivation"){
				dotKey = event.data[0];
				if(!waitingForBtcKey){
					unsubscribe();
				}
				console.log("Found DOT AggKey");
				waitingForDotKey = false;
			}
			if(event.section === "bitcoinVault" && event.method === "AwaitingGovernanceActivation"){
				btcKey = event.data[0];
				if(!waitingForDotKey){
					unsubscribe();
				}
				console.log("Found BTC AggKey");
				waitingForBtcKey = false;
			}
		});
	});
	while(waitingForBtcKey || waitingForDotKey) {
		await new Promise(r => setTimeout(r, 1000));
	};
	const dotKeyAddress = keyring.encodeAddress(dotKey, 0);

	// Step 3
	console.log("Transferring 100 DOT to Polkadot AggKey");
	await polkadot.tx.balances.transfer(dotKeyAddress, 1000000000000).signAndSend(alice);

	// Step 4
	console.log("Requesting Polkadot Vault creation");
	let createCommand = chainflip.tx.environment.createPolkadotVault(dotKey);
	let mytx = chainflip.tx.governance.proposeGovernanceExtrinsic(createCommand);
	await mytx.signAndSend(snowwhite);

	// Step 5
	console.log("Waiting for Vault address on Polkadot chain");
	var vaultAddress;
	var vaultBlock;
	var vaultEventIndex;
	waitingForEvent = true;
	unsubscribe = await polkadot.rpc.chain.subscribeNewHeads(async (header) => {
		const events = await polkadot.query.system.events.at(header.hash);
		events.forEach((record, index) => {
			const {event, phase} = record;
			if(event.section === "proxy" && event.method === "PureCreated"){
				vaultAddress = event.data[0];
				vaultBlock = header.number;
				vaultEventIndex = index;
				unsubscribe();
				waitingForEvent = false;
			}
		});
	});
	while(waitingForEvent) {
		await new Promise(r => setTimeout(r, 1000));
	};
	console.log("Found DOT Vault with address " + vaultAddress);

	// Step 7
	console.log("Transferring 100 DOT to Polkadot Vault");
	await polkadot.tx.balances.transfer(vaultAddress, 1000000000000).signAndSend(alice);

	// Step 8
	console.log("Registering Vaults with state chain");
	const txid = { blockNumber: vaultBlock, extrinsicIndex: vaultEventIndex };
	let dotWitnessing = chainflip.tx.environment.witnessPolkadotVaultCreation(vaultAddress, dotKey, txid, 1);
	myDotTx = chainflip.tx.governance.proposeGovernanceExtrinsic(dotWitnessing);
	await myDotTx.signAndSend(snowwhite, {nonce: -1});

	let btcWitnessing = chainflip.tx.environment.witnessCurrentBitcoinBlockNumberForKey(1, btcKey);
	myBtcTx = chainflip.tx.governance.proposeGovernanceExtrinsic(btcWitnessing);
	await myBtcTx.signAndSend(snowwhite, {nonce: -1});

	// Confirmation
	console.log("Waiting for new epoch");
	waitingForEvent = true;
	unsubscribe = await chainflip.query.system.events((events) => {
		events.forEach((record) => {
			const {event, phase} = record;
			if(event.section === "validator" && event.method === "NewEpoch"){
				unsubscribe();
				waitingForEvent = false;
			}
		});
	});
	while(waitingForEvent) {
		await new Promise(r => setTimeout(r, 1000));
	};
	console.log("=== Vault Setup completed ===");
	process.exit(0);
}

main().catch((error) => {
	console.error(error);
	process.exit(-1);
});