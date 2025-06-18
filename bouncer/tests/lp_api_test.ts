import { InternalAssets as Assets } from '@chainflip/cli';
import assert from 'assert';
import {
  isValidHexHash,
  isValidEthAddress,
  amountToFineAmount,
  sleep,
  observeBalanceIncrease,
  chainFromAsset,
  isWithinOnePercent,
  assetDecimals,
  stateChainAssetFromAsset,
  Chain,
  handleSubstrateError,
  shortChainFromAsset,
  newAddress,
  createStateChainKeypair,
} from 'shared/utils';
import { lpApiRpc } from 'shared/json_rpc';
import { depositLiquidity } from 'shared/deposit_liquidity';
import { sendEvmNative } from 'shared/send_evm';
import { getBalance } from 'shared/get_balance';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { TestContext } from 'shared/utils/test_context';
import { Logger, loggerChild } from 'shared/utils/logger';

type RpcAsset = {
  asset: string;
  chain: Chain;
};

const testAsset = Assets.Eth; // TODO: Make these tests work with any asset
const testRpcAsset: RpcAsset = {
  chain: chainFromAsset(testAsset),
  asset: stateChainAssetFromAsset(testAsset),
};
const testAmount = 0.1;
const testAssetAmount = parseInt(
  amountToFineAmount(testAmount.toString(), assetDecimals(testAsset)),
);
const amountToProvide = testAmount * 50; // Provide plenty of the asset for the tests
const testAddress = '0x1594300cbd587694affd70c933b9ee9155b186d9';

async function provideLiquidityAndTestAssetBalances(logger: Logger) {
  const fineAmountToProvide = parseInt(
    amountToFineAmount(amountToProvide.toString(), assetDecimals('Eth')),
  );
  // We have to wait finalization here because the LP API server is using a finalized block stream (This may change in PRO-777 PR#3986)
  await depositLiquidity(logger, testAsset, amountToProvide, true, '//LP_API');

  // Wait for the LP API to get the balance update, just incase it was slower than us to see the event.
  let retryCount = 0;
  let ethBalance = 0;
  do {
    const balances = await lpApiRpc(logger, `lp_asset_balances`, []);
    ethBalance = parseInt(balances.Ethereum.Eth);
    retryCount++;
    if (retryCount > 14) {
      throw new Error(
        `Failed to provide ${testAsset} for tests (${fineAmountToProvide}). balances: ${JSON.stringify(
          balances,
        )}`,
      );
    }
    await sleep(1000);
  } while (ethBalance < fineAmountToProvide);
}

async function testRegisterLiquidityRefundAddress(parentLogger: Logger) {
  const logger = loggerChild(parentLogger, 'testRegisterLiquidityRefundAddress');
  const observeRefundAddressRegisteredEvent = observeEvent(
    logger,
    'liquidityProvider:LiquidityRefundAddressRegistered',
    {
      test: (event) => event.data.address.Eth === testAddress,
    },
  );

  const registerRefundAddress = await lpApiRpc(logger, `lp_register_liquidity_refund_address`, [
    'Ethereum',
    testAddress,
  ]);
  if (!isValidHexHash(await registerRefundAddress)) {
    throw new Error(`Unexpected lp_register_liquidity_refund_address result`);
  }
  await observeRefundAddressRegisteredEvent.event;

  // TODO: Check that the correct address is now set on the SC
}

async function testLiquidityDepositLegacy(logger: Logger) {
  const observeLiquidityDepositAddressReadyEvent = observeEvent(
    logger,
    'liquidityProvider:LiquidityDepositAddressReady',
    {
      test: (event) => event.data.depositAddress.Eth,
    },
  ).event;

  await assert.rejects(
    () => lpApiRpc(logger, `lp_request_liquidity_deposit_address`, [testRpcAsset, 'InBlock']),
    (e: Error) => e.message.includes('InBlock waiting is not allowed for this method'),
    `Unexpected lp_request_liquidity_deposit_address result. Expected to return an error because InBlock waiting is not allowed`,
  );

  const liquidityDepositAddress = (
    await lpApiRpc(logger, `lp_request_liquidity_deposit_address`, [testRpcAsset, 'Finalized'])
  ).tx_details.response.deposit_address;
  const liquidityDepositEvent = await observeLiquidityDepositAddressReadyEvent;

  assert.strictEqual(
    liquidityDepositEvent.data.depositAddress.Eth,
    liquidityDepositAddress,
    `Incorrect deposit address`,
  );
  assert(
    isValidEthAddress(liquidityDepositAddress),
    `Invalid deposit address: ${liquidityDepositAddress}`,
  );

  // Send funds to the deposit address and watch for deposit event
  const observeAccountCreditedEvent = observeEvent(logger, 'assetBalances:AccountCredited', {
    test: (event) =>
      event.data.asset === testAsset &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(testAssetAmount),
      ),
  }).event;
  await sendEvmNative(
    logger,
    chainFromAsset(testAsset),
    liquidityDepositAddress,
    String(testAmount),
  );
  await observeAccountCreditedEvent;
}

async function testLiquidityDeposit(logger: Logger) {
  const observeLiquidityDepositAddressReadyEvent = observeEvent(
    logger,
    'liquidityProvider:LiquidityDepositAddressReady',
    {
      test: (event) => event.data.depositAddress.Eth,
    },
  ).event;

  const liquidityDepositAddress = (
    await lpApiRpc(logger, `lp_request_liquidity_deposit_address_v2`, [testRpcAsset])
  ).response.deposit_address;
  const liquidityDepositEvent = await observeLiquidityDepositAddressReadyEvent;

  assert.strictEqual(
    liquidityDepositEvent.data.depositAddress.Eth,
    liquidityDepositAddress,
    `Incorrect deposit address`,
  );
  assert(
    isValidEthAddress(liquidityDepositAddress),
    `Invalid deposit address: ${liquidityDepositAddress}`,
  );

  // Send funds to the deposit address and watch for deposit event
  const observeAccountCreditedEvent = observeEvent(logger, 'assetBalances:AccountCredited', {
    test: (event) =>
      event.data.asset === testAsset &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(testAssetAmount),
      ),
  }).event;
  await sendEvmNative(
    logger,
    chainFromAsset(testAsset),
    liquidityDepositAddress,
    String(testAmount),
  );
  await observeAccountCreditedEvent;

  // Also test the old liquidity deposit RPC (must be tested sequentially)
  await testLiquidityDepositLegacy(logger);
}

async function testWithdrawAsset(logger: Logger) {
  const oldBalance = await getBalance(testAsset, testAddress);

  const result = await lpApiRpc(logger, `lp_withdraw_asset`, [
    testAssetAmount,
    testRpcAsset,
    testAddress,
    'InBlock',
  ]);
  const [chain, egressId] = result.tx_details.response;

  assert.strictEqual(chain, testRpcAsset.chain, `Unexpected withdraw asset result`);
  assert(egressId > 0, `Unexpected egressId: ${egressId}`);

  await observeBalanceIncrease(logger, testAsset, testAddress, oldBalance);
}

async function testTransferAsset(logger: Logger) {
  await using chainflip = await getChainflipApi();
  const amountToTransfer = testAssetAmount.toString(16);

  const getLpBalance = async (account: string) =>
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ((await chainflip.query.assetBalances.freeBalances(account, testAsset)) as any).toBigInt();

  const sourceLpAccount = createStateChainKeypair('//LP_API');
  const destinationLpAccount = createStateChainKeypair('//LP_2');

  // Destination account needs a refund address too.
  const chain = shortChainFromAsset(testAsset);
  const refundAddress = await newAddress(testAsset, '//LP_2');
  await chainflip.tx.liquidityProvider
    .registerLiquidityRefundAddress({ [chain]: refundAddress })
    .signAndSend(destinationLpAccount, { nonce: -1 }, handleSubstrateError(chainflip));

  const oldBalanceSource = await getLpBalance(sourceLpAccount.address);
  const oldBalanceDestination = await getLpBalance(destinationLpAccount.address);

  const result = await lpApiRpc(logger, `lp_transfer_asset`, [
    amountToTransfer,
    testRpcAsset,
    destinationLpAccount.address,
  ]);

  let newBalancesSource = await getLpBalance(sourceLpAccount.address);
  let newBalanceDestination = await getLpBalance(destinationLpAccount.address);

  // Wait max for 18 seconds aka 3 blocks for the balances to update.
  for (let i = 0; i < 18; i++) {
    if (newBalanceDestination !== oldBalanceDestination && newBalancesSource !== oldBalanceSource) {
      break;
    }

    await sleep(1000);

    newBalancesSource = await getLpBalance(sourceLpAccount.address);
    newBalanceDestination = await getLpBalance(destinationLpAccount.address);
  }

  // Expect result to be a block hash
  assert.match(result, /0x[0-9a-fA-F]{64}/, `Unexpected transfer asset result`);

  assert(
    newBalanceDestination > oldBalanceDestination,
    `Failed to observe balance increase after transfer for destination account!`,
  );

  assert(
    newBalancesSource < oldBalanceSource,
    `Failed to observe balance decrease after transfer for source account!`,
  );
}

async function testRegisterWithExistingLpAccount(logger: Logger) {
  try {
    await lpApiRpc(logger, `lp_register_account`, []);
    throw new Error(`Unexpected lp_register_account result`);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } catch (error: any) {
    // This account is already registered, so the command will fail.
    // This message is from the `AccountRoleAlreadyRegistered` pallet error.
    if (!error.message.includes('The account already has a registered role')) {
      throw new Error(`Unexpected lp_register_account error: ${error}`);
    }
  }
}

/// Test lp_set_range_order and lp_update_range_order by minting, updating, and burning a range order.
async function testRangeOrder(logger: Logger) {
  const range = { start: 1, end: 2 };
  const orderId = 74398; // Arbitrary order id so it does not interfere with other tests
  const zeroAssetAmounts = {
    AssetAmounts: {
      maximum: { base: 0, quote: 0 },
      minimum: { base: 0, quote: 0 },
    },
  };

  // Cleanup after any unfinished previous test so it does not interfere with this test
  await lpApiRpc(logger, `lp_set_range_order`, [
    testRpcAsset,
    'USDC',
    orderId,
    range,
    zeroAssetAmounts,
    'InBlock',
  ]);

  // Mint a range order
  const mintRangeOrder = (
    await lpApiRpc(logger, `lp_set_range_order`, [
      testRpcAsset,
      'USDC',
      orderId,
      range,
      {
        AssetAmounts: {
          maximum: { base: testAssetAmount, quote: 0 },
          minimum: { base: 0, quote: 0 },
        },
      },
      'InBlock',
    ])
  ).tx_details.response;
  assert(mintRangeOrder.length >= 1, `Empty mint range order result`);
  assert(
    parseInt(mintRangeOrder[0].liquidity_total) > 0,
    `Expected range order to have liquidity after mint`,
  );

  // Update the range order
  const updateRangeOrder = (
    await lpApiRpc(logger, `lp_update_range_order`, [
      testRpcAsset,
      'USDC',
      orderId,
      range,
      {
        increase: {
          AssetAmounts: {
            maximum: { base: testAssetAmount, quote: 0 },
            minimum: { base: 0, quote: 0 },
          },
        },
      },
      'InBlock',
    ])
  ).tx_details.response;

  assert(updateRangeOrder.length >= 1, `Empty update range order result`);
  let matchUpdate = false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  updateRangeOrder.forEach((order: any) => {
    const liquidityIncrease = order.size_change?.increase?.liquidity ?? 0;
    if (liquidityIncrease > 0 && parseInt(order.liquidity_total) > 0) {
      matchUpdate = true;
    }
  });
  assert.strictEqual(matchUpdate, true, `Expected update of range order to increase liquidity`);

  // Burn the range order
  const burnRangeOrder = (
    await lpApiRpc(logger, `lp_set_range_order`, [
      testRpcAsset,
      'USDC',
      orderId,
      range,
      zeroAssetAmounts,
      'InBlock',
    ])
  ).tx_details.response;

  assert(burnRangeOrder.length >= 1, `Empty burn range order result`);
  let matchBurn = false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  burnRangeOrder.forEach((order: any) => {
    const liquidityDecrease = order.size_change?.decrease?.liquidity ?? 0;
    if (liquidityDecrease > 0 && parseInt(order.liquidity_total) === 0) {
      matchBurn = true;
    }
  });
  assert.strictEqual(matchBurn, true, `Expected burn of range order to decrease liquidity to 0`);
}

async function testGetOpenSwapChannels(logger: Logger) {
  // TODO: Test with some SwapChannelInfo data
  const openSwapChannels = await lpApiRpc(logger, `lp_get_open_swap_channels`, []);
  assert(openSwapChannels.ethereum, `Missing ethereum swap channel info`);
  assert(openSwapChannels.polkadot, `Missing polkadot swap channel info`);
  assert(openSwapChannels.bitcoin, `Missing bitcoin swap channel info`);
}

/// Test lp_set_limit_order and lp_update_limit_order by minting, updating, and burning a limit order.

async function testLimitOrder(logger: Logger) {
  const orderId = 98432; // Arbitrary order id so it does not interfere with other tests
  const tick = 2;

  // Cleanup after any unfinished previous test so it does not interfere with this test
  await lpApiRpc(logger, `lp_set_limit_order`, [testRpcAsset, 'USDC', 'sell', orderId, tick, 0]);

  // Mint a limit order
  const mintLimitOrder = (
    await lpApiRpc(logger, `lp_set_limit_order`, [
      testRpcAsset,
      'USDC',
      'sell',
      orderId,
      tick,
      testAssetAmount,
    ])
  ).tx_details.response;
  assert(mintLimitOrder.length >= 1, `Empty mint limit order result`);
  assert(
    parseInt(mintLimitOrder[0].sell_amount_change.increase) > 0,
    `Expected mint of limit order to increase liquidity. sell_amount_change: ${JSON.stringify(
      mintLimitOrder[0].sell_amount_change,
    )}`,
  );
  assert.strictEqual(
    parseInt(mintLimitOrder[0].sell_amount_total),
    testAssetAmount,
    `Unexpected amount of asset was minted for limit order`,
  );

  // Update the limit order
  const updateLimitOrder = (
    await lpApiRpc(logger, `lp_update_limit_order`, [
      testRpcAsset,
      'USDC',
      'sell',
      orderId,
      tick,
      {
        increase: testAssetAmount,
      },
    ])
  ).tx_details.response;

  assert(updateLimitOrder.length >= 1, `Empty update limit order result`);
  let matchUpdate = false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  updateLimitOrder.forEach((order: any) => {
    if (
      parseInt(order.sell_amount_change.increase) === testAssetAmount &&
      parseInt(order.sell_amount_total) === testAssetAmount * 2
    ) {
      matchUpdate = true;
    }
  });
  assert.strictEqual(
    matchUpdate,
    true,
    `Expected update of limit order to increase liquidity to exact amount`,
  );

  // Burn the limit order
  const burnLimitOrder = (
    await lpApiRpc(logger, `lp_set_limit_order`, [testRpcAsset, 'USDC', 'sell', orderId, tick, 0])
  ).tx_details.response;

  assert(burnLimitOrder.length >= 1, `Empty burn limit order result`);
  let matchBurn = false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  burnLimitOrder.forEach((order: any) => {
    if (
      parseInt(order.sell_amount_change.decrease) === testAssetAmount * 2 &&
      parseInt(order.sell_amount_total) === 0
    ) {
      matchBurn = true;
    }
  });
  assert.strictEqual(matchBurn, true, `Expected burn of limit order to decrease liquidity to 0`);
}

async function testInternalSwap(logger: Logger) {
  const lp = createStateChainKeypair('//LP_API');

  // Start an on chain swap
  const swapRequestId = (
    await lpApiRpc(logger, `lp_schedule_swap`, [
      testAssetAmount,
      testRpcAsset,
      'USDC',
      0, // retry duration
      '0x0', // minimum price
    ])
  ).tx_details.response.swap_request_id;
  logger.debug(`On chain swap request id: ${swapRequestId}`);
  assert(swapRequestId > 0, 'Unexpected on chain swap request id');

  // Wait for the swap to complete
  await observeEvent(logger, 'swapping:CreditedOnChain', {
    test: (event) =>
      event.data.accountId === lp.address &&
      Number(event.data.swapRequestId.replaceAll(',', '')) === swapRequestId,
    historicalCheckBlocks: 3,
  });
}

/// Runs all of the LP commands via the LP API Json RPC Server that is running and checks that the returned data is as expected
export async function testLpApi(testContext: TestContext) {
  // Provide the amount of liquidity needed for the tests
  await provideLiquidityAndTestAssetBalances(testContext.logger);

  await Promise.all([
    testRegisterLiquidityRefundAddress(testContext.logger),
    testLiquidityDeposit(testContext.logger),
    testWithdrawAsset(testContext.logger),
    testRegisterWithExistingLpAccount(testContext.logger),
    testRangeOrder(testContext.logger),
    testLimitOrder(testContext.logger),
    testGetOpenSwapChannels(testContext.logger),
    testInternalSwap(testContext.logger),
  ]);

  await testTransferAsset(testContext.logger);
}
