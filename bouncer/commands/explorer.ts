#!/usr/bin/env -S pnpm tsx

// Call with a range of blocks to query, like:
// ./explorer.js 1234 1300
// Alternatively, the first argument can be the string "live" to query the latest blocks.
// In that case, the second argument specifying the last block to report is optional:
// ./explorer.ts live
//    or
// ./explorer.ts live 5000
//
// It can be convenient to filter out annoying events using grep, for example:
// ./explorer.ts live | grep -Fv -e "ExtrinsicSuccess" -e "update_chain_state" -e "TransactionFeePaid" -e "heartbeat" -e "timestamp"
// or you can show only lines that contain one of your specified words:
// ./explorer.ts live | grep -F -e "Block" -e "ChainStateUpdated"

import { getChainflipApi } from "../shared/utils";
import { ApiPromise } from "@polkadot/api";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function args_to_string(args: any): string{
// eslint-disable-next-line @typescript-eslint/no-explicit-any
	return args.map((a: any) => {
		if(a.meta !== undefined && a.args !== undefined){
			return `${a.section.toString()}.${a.meta.name.toString()}(${args_to_string(a.args)})`;
		} else {
			return a.toString();
		}
	}).join(', ');
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function print_extrinsic(ex: any){
	const { isSigned, meta, method: { args, method, section } } = ex.extrinsic;
	console.log(`  Extrinsic ${ex.block}-${ex.index}: ${section}.${method}(${args_to_string(args)})`);
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function print_events_for_extrinsic_id(events: any, ex_id: number, blockNum: number){
// eslint-disable-next-line @typescript-eslint/no-explicit-any
	events.forEach((event: any, i: number) => {
		let decoded_event = event.toHuman()!;
		if(Number(decoded_event.phase.ApplyExtrinsic) === Number(ex_id) || (ex_id === -1 && (decoded_event.phase === 'Initialization' || decoded_event.phase === 'Finalization'))){
			const tag = ex_id === -1 ? decoded_event.phase : ex_id;
			console.log(`    Event ${blockNum}-${tag}-${i}: ${decoded_event.event.section}.${decoded_event.event.method}(${args_to_string(event.event.data)})`);
		} 
	});
}

async function process_block(blockNumber: number, api: ApiPromise){
	const blockHash = await api.rpc.chain.getBlockHash(blockNumber);
	const signedBlock = await api.rpc.chain.getBlock(blockHash);
	const events = await (await api.at(blockHash)).query.system.events();
	console.log();
	console.log(`Block ${blockNumber}:`);
	print_events_for_extrinsic_id(events, -1, blockNumber);
	signedBlock.block.extrinsics.forEach((ex, i) => {
		print_extrinsic({block: blockNumber, index: i, extrinsic: ex});
		print_events_for_extrinsic_id(events, i, blockNumber);
	});
}

async function main() {	
	const start = process.argv[2];
	const end = process.argv[3];
	const api = await getChainflipApi();
	if(start === 'live'){
		const unsubscribe = await api.rpc.chain.subscribeNewHeads(async (header) => {
			if(end && parseInt(end) < header.number.toNumber()){
				unsubscribe();
				process.exit(0);
			}
			await process_block(header.number.toNumber(), api);
	    });
	} else {
		for(var blocknum=parseInt(start); blocknum<=parseInt(end); blocknum++){
			await process_block(blocknum, api);
		}
		process.exit(0);
	}
}

main();