#!/usr/bin/env -S pnpm tsx
import { requestNewSwap } from '../shared/perform_swap';
import { setMinimumDeposit } from '../shared/set_minimum_deposit';
import { observeEvent } from '../shared/utils/substrate';
import { sendDot } from '../shared/send_dot';

async function main() {
  await setMinimumDeposit('Dot', BigInt(200000000000));
  console.log('Set minimum deposit to 20 DOT');
  const depositAddress = (
    await requestNewSwap('Dot', 'Eth', '0xd92bd8c144b8edba742b07909c04f8b93d875d93')
  ).depositAddress;
  const depositIgnored = observeEvent(':DepositIgnored');
  await sendDot(depositAddress, '19');
  console.log('Sent 19 DOT');
  await depositIgnored.event;
  console.log('Deposit was ignored');
  const depositSuccess = observeEvent(':DepositFinalised');
  await sendDot(depositAddress, '21');
  console.log('Sent 21 DOT');
  await depositSuccess.event;
  console.log('Deposit was successful');
  await setMinimumDeposit('Dot', BigInt(0));
  console.log('Reset minimum deposit to 0 DOT');
}

try {
  console.log('=== Testing minimum deposit ===');
  await main();
  console.log('=== Test complete ===');
  process.exit(0);
} catch (e) {
  console.error(e);
  process.exit(-1);
}
