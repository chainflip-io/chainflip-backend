#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes no arguments.
// It will force a rotation on the chainflip state-chain
// For example: ./commands/vault_rotation.sh

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const snowwhite_uri = process.env.SNOWWHITE_URI || 'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
	const snowwhite = keyring.createFromUri(snowwhite_uri);
	const chainflip = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});

	console.log("Forcing rotation");
	await chainflip.tx.governance.proposeGovernanceExtrinsic(chainflip.tx.validator.forceRotation()).signAndSend(snowwhite);

	process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
	console.error(error);
	process.exit(-1);
});
