// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a new liquidity pool for the given currency and
// initial price in USDC
// For example: ./commands/create_pool.sh btc 10000

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { observeEvent, getChainflipApi, handleSubstrateError } from '../shared/utils';
import { runWithTimeout } from '../shared/utils';

const decimals = new Map<string, number>([
	["dot", 10],
	["eth", 18],
	["btc", 8],
	["usdc", 6],
	["flip", 18]
]);

async function createLpPool(){
	const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';
	const ccy = process.argv[2];
	const initial_price = parseFloat(process.argv[3]);
	const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
	await cryptoWaitReady();

	const keyring = new Keyring({type: 'sr25519'});
	const snowwhite_uri = process.env.SNOWWHITE_URI || 'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';
	const snowwhite = keyring.createFromUri(snowwhite_uri);

	const price = BigInt(Math.round(Math.sqrt(initial_price/Math.pow(10, decimals.get(ccy)!-decimals.get("usdc")!))*Math.pow(2,96)));
	console.log("Setting up " + ccy + " pool with an initial price of " + initial_price + " usdc/" + ccy);
	let event = observeEvent('liquidityPools:NewPoolCreated', chainflip, (data) => {
		return data[0].toLowerCase() == ccy;
	});
	await chainflip.tx.governance.proposeGovernanceExtrinsic(chainflip.tx.liquidityPools.newPool(ccy, 0, price)).signAndSend(snowwhite, {nonce: -1}, handleSubstrateError(chainflip));
	await event;
	process.exit(0);
}

runWithTimeout(createLpPool(), 20000).catch((error) => {
	console.error(error);
	process.exit(-1);
  });