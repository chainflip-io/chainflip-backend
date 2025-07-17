#!/usr/bin/env -S pnpm tsx

// Usage:
//   ./elections_inspection.ts <block_number>
//   ./elections_inspection.ts live 
//
// Prints all elections.vote extrinsics for the given block or for each new block (if 'live'). These are grouped by election id and vote 
// such that it easy to have a nice overview over what validators are voting
//
// Set the node endpoint with CF_NODE_ENDPOINT env var if needed.

import { ApiPromise } from '@polkadot/api';
import { getChainflipApi } from 'shared/utils/substrate';

let votes: Map<string, Map<string,number>>;
let vote_to_partial: Map<string, string>;

function printVoteSummary(votes: Map<string, Map<string, number>>) {
    if (votes.size === 0) {
        console.log("No votes found.");
        return;
    }
    for (const [election, voteMap] of votes.entries()) {
        console.log(`${election}`);
        for (const [voteType, count] of voteMap.entries()) {
            console.log(`   ${voteType}: ${count}`);
        }
        console.log(); // extra line for readability
    }
}

function printVotes(block: number, extrinsics: any[]) {
    votes = new Map();
    vote_to_partial = new Map();

    for (const ex of extrinsics) {
        const parse_ex = ex.ex.toHuman().method;;
        const authority_votes = parse_ex.args.authority_votes;
        // console.log(parse_ex.args.authority_votes);
        if (authority_votes && typeof authority_votes === 'object' ) {
            for (const [key, value] of Object.entries(authority_votes)) {
                // console.log(`Key: ${key}, Value: ${JSON.stringify(value)}`);
                if (value && typeof value === 'object') {
                    for (const [vote_or_partial, valueType] of Object.entries(value)) {
                            // console.log(valueType);
                            for (const [_, vaaaaaaaa] of Object.entries(valueType)) {
                                // console.log(vaaaaaaaa);
                                if(!votes.has(parse_ex.section + key)) {
                                    votes.set(parse_ex.section + key, new Map());
                                } 
                                const innerMap = votes.get(parse_ex.section + key);
                                innerMap?.set(JSON.stringify(vaaaaaaaa), (innerMap.get(JSON.stringify(vaaaaaaaa)) || 0) + 1);
                            }
                    }
                }
            }
        }
    }
    printVoteSummary(votes);
}

async function processBlock(blockNumber: number, api: ApiPromise) {
  const blockHash = await api.rpc.chain.getBlockHash(blockNumber);
  const signedBlock = await api.rpc.chain.getBlock(blockHash);
  const voteExtrinsics = signedBlock.block.extrinsics
    .map((ex, i) => ({ ex, i }))
    .filter(({ ex }) =>
      ex.method.method === 'vote'
    );
  if (voteExtrinsics.length > 0) {
    console.log(`\nBlock ${blockNumber}:`);
    printVotes(blockNumber, voteExtrinsics);
  }
}

async function main() {
  const start = process.argv[2];
  const api = await getChainflipApi();
  if (start === 'live') {
    await api.rpc.chain.subscribeFinalizedHeads(async (header) => {
      await processBlock(header.number.toNumber(), api);
    });
  } else {
    const blocknum = parseInt(start);
    if (isNaN(blocknum)) {
      console.error('Please provide a block number or "live"');
      process.exit(1);
    }
    await processBlock(blocknum, api);
    process.exit(0);
  }
}

await main();
