#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will download the runtime from the governance proposal with the ID given by the first
// argument and save it into a file named "proposed_runtime.wasm"
//
// For example: ./commands/get_proposed_runtime.ts 123

import { getChainflipApi } from '../shared/utils';
import fs from 'fs';
const proposal_id = process.argv[2];
const api = await getChainflipApi();
let proposal = await api.query.governance.proposals(proposal_id);
let call = proposal.unwrap().call;
let extrinsic = api.registry.createType('Call', proposal.unwrap().call);
await fs.writeFile("proposed_runtime.wasm", extrinsic.args[1].toU8a(), (err) => {if(err)console.log("error!"); else process.exit(0);});