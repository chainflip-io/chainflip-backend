#!/usr/bin/env node

// INSTRUCTIONS
//
// This command takes one argument.
// It will trigger an EthereumBroadcaster signing stress test to be executed on the chainflip state-chain
// The argument specifies the number of requested signatures
// For example: ./commands/stress_test.sh 3
// will initiate a stress test generating 3 signatures

const { ApiPromise, WsProvider } = require('@polkadot/api');
const { Keyring } = require('@polkadot/keyring');
const { cryptoWaitReady } = require('@polkadot/util-crypto');
const { runWithTimeout } = require('../shared/utils');

async function main() {
	const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	const signatures_count = process.argv[2];
	await cryptoWaitReady();
	const keyring = new Keyring({type: 'sr25519'});
	const snowwhite_uri = process.env.SNOWWHITE_URI || 'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
	const snowwhite = keyring.createFromUri(snowwhite_uri);
	const api = await ApiPromise.create({provider: new WsProvider(cf_node_endpoint), noInitWarn: true});
	const stressTest = api.tx.ethereumBroadcaster.stressTest(signatures_count);
	const sudoCall = api.tx.governance.callAsSudo(stressTest);
	const proposal = api.tx.governance.proposeGovernanceExtrinsic(sudoCall);
	await proposal.signAndSend(snowwhite);
	process.exit(0);
}

runWithTimeout(main(), 10000).catch((error) => {
	console.error(error);
	process.exit(-1);
});