#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will download the runtime from the governance proposal with the ID given by the first
// argument and save it into a file named "proposed_runtime.wasm"
//
// For example: ./commands/get_proposed_runtime.ts 123

import fs from 'fs';
import { getChainflipApi } from '../shared/utils';

const proposalId = process.argv[2];
const api = await getChainflipApi();
const proposal = await api.query.governance.proposals(proposalId);
const extrinsic = api.registry.createType('Call', proposal.unwrap().call);
await fs.writeFile('proposed_runtime.wasm', extrinsic.args[1].toU8a(), (err) => {
  if (err) console.log('error!');
  else process.exit(0);
});
