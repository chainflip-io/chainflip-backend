// INSTRUCTIONS
//
// This command takes two arguments.
// It will create a zero to infinity range order for the currency and amount given
// For example: pnpm tsx ./commands/range_order.ts btc 10

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { observeEvent, getChainflipApi, runWithTimeout, handleSubstrateError } from '../shared/utils';

const cf_node_endpoint = process.env.CF_NODE_ENDPOINT || 'ws://127.0.0.1:9944';

const decimals = new Map<string, number>([
	["dot", 10],
	["eth", 18],
	["btc", 8],
	["usdc", 6],
	["flip", 18]
]);

async function range_order(){
	const ccy = process.argv[2];
	const amount = process.argv[3].trim();
	var fine_amount = '';
	if(amount.indexOf('.') == -1){
		fine_amount = amount + "0".repeat(decimals.get(ccy)!);
	} else {
		const amount_parts = amount.split('.');
		fine_amount = amount_parts[0] + amount_parts[1].padEnd(decimals.get(ccy)!,'0').substr(0, decimals.get(ccy)!);
	}
	const liquidity = Math.sqrt(Number(fine_amount))
	const chainflip = await getChainflipApi(process.env.CF_NODE_ENDPOINT);
	await cryptoWaitReady();

	const keyring = new Keyring({type: 'sr25519'});
	keyring.setSS58Format(2112);
	const lp_uri = process.env.LP_URI || '//LP_1';
	const lp = keyring.createFromUri(lp_uri);

	const current_sqrt_price = (await chainflip.query.liquidityPools.pools(ccy)).toJSON().poolState.rangeOrders.currentSqrtPrice;
	const price = Math.round(current_sqrt_price/Math.pow(2,96)*Number(fine_amount));
	console.log("Setting up " + ccy + " range order");
	await chainflip.tx.liquidityPools.collectAndMintRangeOrder(ccy, [-887272, 887272], price).signAndSend(lp, {nonce: -1}, handleSubstrateError(chainflip));
    await observeEvent('liquidityPools:RangeOrderMinted', chainflip, (data) => {
		return data[0] == lp.address && data[1].toLowerCase() == ccy;
	});
	process.exit(0);
}

runWithTimeout(range_order(), 120000).catch((error) => {
	console.error(error);
	process.exit(-1);
});