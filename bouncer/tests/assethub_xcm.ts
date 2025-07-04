import { TestContext } from 'shared/utils/test_context';
import { performSwap } from 'shared/perform_swap';
import { observeBalanceIncrease } from 'shared/utils';
import { getBalance } from 'shared/get_balance';

export async function testAssethubXcm(testContext: TestContext, _seed?: string) {
  const metadata = {
    message:
      '0x1f0103010003000101003e1420e52818eceb728bce3ab8dc71b750a824d3959eb3c449626ea786a8803d0304000100000700743ba40b00000000',
    gasBudget: '0',
    ccmAdditionalData: '0x',
  };
  const oldHubBalance = await getBalance(
    'HubDot',
    '14iGgWDpriDToidv1GY28a8yGqF4DyR397euELCzQB87qbRM',
  );
  const oldDotBalance = await getBalance('Dot', '12QPwzxiXa1UAsgeoeNvvPnJqCFE8SwDb4FVXWauYWCwRiHt');
  await Promise.all([
    performSwap(
      testContext.logger,
      'Btc',
      'HubDot',
      '14iGgWDpriDToidv1GY28a8yGqF4DyR397euELCzQB87qbRM',
      metadata,
    ),
    observeBalanceIncrease(
      testContext.logger,
      'HubDot',
      '14iGgWDpriDToidv1GY28a8yGqF4DyR397euELCzQB87qbRM',
      oldHubBalance,
    ),
    observeBalanceIncrease(
      testContext.logger,
      'Dot',
      '12QPwzxiXa1UAsgeoeNvvPnJqCFE8SwDb4FVXWauYWCwRiHt',
      oldDotBalance,
    ),
  ]);
}
