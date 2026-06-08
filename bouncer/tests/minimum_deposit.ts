import { requestNewSwap } from 'shared/perform_swap';
import { setMinimumDeposit } from 'shared/set_minimum_deposit';
import { observeEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { sendHubAsset } from 'shared/send_hubasset';
import { newChainflipIO } from 'shared/utils/chainflip_io';

export async function testMinimumDeposit(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await setMinimumDeposit(cf.logger, 'HubDot', BigInt(200000000000));
  cf.debug('Set minimum deposit to 20 DOT');
  const depositAddress = (
    await requestNewSwap(cf, 'HubDot', 'Eth', '0xd92bd8c144b8edba742b07909c04f8b93d875d93')
  ).depositAddress;
  const depositFailed = observeEvent(cf.logger, ':DepositFailed');
  await sendHubAsset(testContext.logger, 'HubDot', depositAddress, '19');
  cf.debug('Sent 19 DOT');
  await depositFailed.event;
  cf.debug('Deposit was ignored');
  const depositSuccess = observeEvent(cf.logger, ':DepositFinalised');
  await sendHubAsset(testContext.logger, 'HubDot', depositAddress, '21');
  cf.debug('Sent 21 DOT');
  await depositSuccess.event;
  cf.debug('Deposit was successful');
  await setMinimumDeposit(cf.logger, 'HubDot', BigInt(0));
  cf.debug('Reset minimum deposit to 0 DOT');
}
