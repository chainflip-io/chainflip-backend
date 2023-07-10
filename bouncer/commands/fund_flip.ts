// INSTRUCTIONS
//
// This command takes two arguments.
// It will fund the public key provided as the first argument with the amount of
// tokens provided in the second argument. The token amount is interpreted as FLIP
//
// For example: pnpm tsx ./commands/fund_flip.ts 0x5f2b0c89b9f7f240c2aab5cc3118f51f8ba7d4dfb9cd2a1abd6ea4d327bcd34c 5.5
// will fund 5.5 FLIP to the account with public key 0x5f2b0c89b9f7f240c2aab5cc3118f51f8ba7d4dfb9cd2a1abd6ea4d327bcd34c
// (That would be account cFL2GAaTbP6UHgfQwJuJ7Naq6gh7ZxZiWQ8EcmdYeopGhpziQ)

import { runWithTimeout } from '../shared/utils';
import { fundFlip } from '../shared/fund_flip';

async function main(): Promise<void> {
  let pubkey = process.argv[2];
  if(pubkey.substr(0,2) != '0x'){
    pubkey = "0x" + pubkey
  }
  const flipAmount = process.argv[3].trim();

  await fundFlip(pubkey, flipAmount);

  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
