import { requestNewSwap } from '../shared/perform_swap';
import { setMinimumDeposit } from '../shared/set_minimum_deposit';
import { observeEvent } from '../shared/utils/substrate';
import { sendDot } from '../shared/send_dot';
import { ExecutableTest } from '../shared/executable_test';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testMinimumDeposit = new ExecutableTest('Minimum-Deposit', main, 250);

async function main() {
  await setMinimumDeposit('Dot', BigInt(200000000000));
  testMinimumDeposit.log('Set minimum deposit to 20 DOT');
  const depositAddress = (
    await requestNewSwap('Dot', 'Eth', '0xd92bd8c144b8edba742b07909c04f8b93d875d93')
  ).depositAddress;
  const depositFailed = observeEvent(':DepositFailed');
  await sendDot(depositAddress, '19');
  testMinimumDeposit.log('Sent 19 DOT');
  await depositFailed.event;
  testMinimumDeposit.log('Deposit was ignored');
  const depositSuccess = observeEvent(':DepositFinalised');
  await sendDot(depositAddress, '21');
  testMinimumDeposit.log('Sent 21 DOT');
  await depositSuccess.event;
  testMinimumDeposit.log('Deposit was successful');
  await setMinimumDeposit('Dot', BigInt(0));
  testMinimumDeposit.log('Reset minimum deposit to 0 DOT');
}
