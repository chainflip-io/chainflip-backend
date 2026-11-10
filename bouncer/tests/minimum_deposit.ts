import { requestNewSwap } from 'shared/perform_swap';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { observeEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { sendHubAsset } from 'shared/send_hubasset';
import { newChainflipIO } from 'shared/utils/chainflip_io';
import { Logger } from 'shared/utils/logger';

async function setHubDotMinimumDeposit(logger: Logger, amount: bigint) {
  const configUpdated = observeEvent(logger, 'IngressEgress:PalletConfigUpdated');
  await submitGovernanceExtrinsic((api) =>
    api.tx.assethubIngressEgress.updatePalletConfig([
      { type: 'SetMinimumDepositAssethub', value: { asset: 'HubDot', minimumDeposit: amount } },
    ]),
  );
  await configUpdated.event;
}

export async function testMinimumDeposit(testContext: TestContext) {
  const cf = await newChainflipIO(testContext.logger, []);
  await setHubDotMinimumDeposit(cf.logger, BigInt(200000000000));
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
  await setHubDotMinimumDeposit(cf.logger, BigInt(0));
  cf.debug('Reset minimum deposit to 0 DOT');
}
