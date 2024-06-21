#!/usr/bin/env -S pnpm tsx
// INSTRUCTIONS
//
// This command takes one argument.
// It will download the runtime from the governance proposal with the ID given by the first
// argument and save it into a file named "proposed_runtime.wasm"
//
// For example: ./commands/get_proposed_runtime.ts 123

import fs from 'fs';
import { getChainflipApi } from '../shared/utils/substrate';

const proposalId = process.argv[2];
const api = await getChainflipApi();
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const proposal: any = await api.query.governance.proposals(proposalId);
const extrinsic = api.registry.createType('Call', proposal.unwrap().call);
const raw = extrinsic.args[1].toU8a();
const indicator: number = raw[0] && 3;
let skip: number;
switch (indicator) {
  case 0: {
    skip = 1;
    break;
  }
  case 1: {
    skip = 2;
    break;
  }
  case 3: {
    skip = 4;
    break;
  }
  case 4: {
    // eslint-disable-next-line no-bitwise
    skip = (raw[0] >> 2) + 4;
    break;
  }
  default: {
    throw new Error('Invalid indicator');
  }
}

fs.writeFile('proposed_runtime.wasm', raw.slice(skip), (err) => {
  if (err) console.log('error!');
  else process.exit(0);
});
