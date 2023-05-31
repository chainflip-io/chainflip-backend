// INSTRUCTIONS
//
// This command takes one argument.
// It will print the Dot balance of the address provided as the first argument.
//
// For example: pnpm tsx ./commands/get_dot_balance.ts 5Dd1drBHuBzHK7qGWzGQ2iR2KnbYZJbYuUfc88v5Cv4juWci
// might print: 1.2

import { ApiPromise, WsProvider } from '@polkadot/api';
import { runWithTimeout } from '../shared/utils';

async function main(): Promise<void> {
  const address = process.argv[2] ?? '0';
  const polkadotEndpoint = process.env.POLKADOT_ENDPOINT ?? 'ws://127.0.0.1:9945';
  const polkadot = await ApiPromise.create({
    provider: new WsProvider(polkadotEndpoint),
    noInitWarn: true,
  });
  const planckBalance: string = (await polkadot.query.system.account(address)).data.free.toString();
  const balanceLen = planckBalance.length;
  let balance;
  if (balanceLen > 10) {
    const decimalLocation = balanceLen - 10;
    balance = planckBalance.slice(0, decimalLocation) + '.' + planckBalance.slice(decimalLocation);
  } else {
    balance = '0.' + planckBalance.padStart(10, '0');
  }
  console.log(balance);
  process.exit(0);
}

runWithTimeout(main(), 5000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
