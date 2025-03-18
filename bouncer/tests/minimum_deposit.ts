import { requestNewSwap } from '../shared/perform_swap';
import { setMinimumDeposit } from '../shared/set_minimum_deposit';
import { observeEvent } from '../shared/utils/substrate';
import { sendDot } from '../shared/send_dot';
import { TestContext } from '../shared/utils/test_context';

export async function testMinimumDeposit(testContext: TestContext) {
  const logger = testContext.logger;
  await setMinimumDeposit(logger, 'Dot', BigInt(200000000000));
  logger.debug('Set minimum deposit to 20 DOT');
  const depositAddress = (
    await requestNewSwap(logger, 'Dot', 'Eth', '0xd92bd8c144b8edba742b07909c04f8b93d875d93')
  ).depositAddress;
  const depositFailed = observeEvent(logger, ':DepositFailed');
  await sendDot(depositAddress, '19');
  logger.debug('Sent 19 DOT');
  await depositFailed.event;
  logger.debug('Deposit was ignored');
  const depositSuccess = observeEvent(logger, ':DepositFinalised');
  await sendDot(depositAddress, '21');
  logger.debug('Sent 21 DOT');
  await depositSuccess.event;
  logger.debug('Deposit was successful');
  await setMinimumDeposit(logger, 'Dot', BigInt(0));
  logger.debug('Reset minimum deposit to 0 DOT');
}
