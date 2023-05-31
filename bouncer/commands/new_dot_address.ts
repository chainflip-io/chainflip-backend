// INSTRUCTIONS
//
// This command takes one argument
// It will take the provided seed turn it into a new polkadot address and return the address
// For example: pnpm tsx ./commands/new_dot_address.ts foobar
// returns: 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci

import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const seed = process.argv[2] ?? '0';
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const { address } = keyring.createFromUri('//' + seed);
  console.log(address);
  process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
