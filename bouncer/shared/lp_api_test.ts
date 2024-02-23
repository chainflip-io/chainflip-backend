import { assetDecimals, Asset, Chain, Assets } from '@chainflip-io/cli';
import assert from 'assert';
import {
  getChainflipApi,
  observeEvent,
  isValidHexHash,
  isValidEthAddress,
  amountToFineAmount,
  sleep,
  observeBalanceIncrease,
  chainFromAsset,
  isWithinOnePercent,
} from './utils';
import { jsonRpc } from './json_rpc';
import { provideLiquidity } from './provide_liquidity';
import { sendEvmNative } from './send_evm';
import { getBalance } from './get_balance';

type RpcAsset = {
  asset: Asset;
  chain: Chain;
};

const testAsset: Asset = 'ETH'; // TODO: Make these tests work with any asset
const testRpcAsset: RpcAsset = { chain: chainFromAsset(testAsset), asset: testAsset };
const testAmount = 0.1;
const testAssetAmount = parseInt(amountToFineAmount(testAmount.toString(), assetDecimals.ETH));
const amountToProvide = testAmount * 50; // Provide plenty of the asset for the tests
const chainflip = await getChainflipApi();
const testAddress = '0x1594300cbd587694affd70c933b9ee9155b186d9';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function lpApiRpc(method: string, params: any[]): Promise<any> {
  // The port for the lp api is defined in `start_lp_api.sh`
  return jsonRpc(method, params, 'http://127.0.0.1:10589');
}

async function provideLiquidityAndTestAssetBalances() {
  const fineAmountToProvide = parseInt(
    amountToFineAmount(amountToProvide.toString(), assetDecimals.ETH),
  );
  // We have to wait finalization here because the LP API server is using a finalized block stream (This may change in PRO-777 PR#3986)
  await provideLiquidity(testAsset, amountToProvide, true);

  // Wait for the LP API to get the balance update, just incase it was slower than us to see the event.
  let retryCount = 0;
  let ethBalance = 0;
  do {
    const balances = await lpApiRpc(`lp_asset_balances`, []);
    ethBalance = parseInt(
      balances.Ethereum.filter((el) => el.asset === 'ETH').map((el) => el.balance)[0],
    );
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

async function testRegisterLiquidityRefundAddress() {
  const observeRefundAddressRegisteredEvent = observeEvent(
    'liquidityProvider:LiquidityRefundAddressRegistered',
    chainflip,
    (event) => event.data.address.Eth === testAddress,
  );

  const registerRefundAddress = await lpApiRpc(`lp_register_liquidity_refund_address`, [
    'Ethereum',
    testAddress,
  ]);
  if (!isValidHexHash(await registerRefundAddress)) {
    throw new Error(`Unexpected lp_register_liquidity_refund_address result`);
  }
  await observeRefundAddressRegisteredEvent;

  // TODO: Check that the correct address is now set on the SC
}

async function testLiquidityDeposit() {
  const observeLiquidityDepositAddressReadyEvent = observeEvent(
    'liquidityProvider:LiquidityDepositAddressReady',
    chainflip,
    (event) => event.data.depositAddress.Eth,
  );

  const liquidityDepositAddress = (
    await lpApiRpc(`lp_liquidity_deposit`, [testRpcAsset, 'InBlock'])
  ).tx_details.response;
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
  const observeAccountCreditedEvent = observeEvent(
    'liquidityProvider:AccountCredited',
    chainflip,
    (event) =>
      event.data.asset.toUpperCase() === testAsset.toUpperCase() &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(testAssetAmount),
      ),
  );
  await sendEvmNative(chainFromAsset(testAsset), liquidityDepositAddress, String(testAmount));
  await observeAccountCreditedEvent;
}

async function testWithdrawAsset() {
  console.log('=== Starting testWithdrawAsset ===');
  const oldBalance = await getBalance(testAsset, testAddress);

  const result = await lpApiRpc(`lp_withdraw_asset`, [
    testAssetAmount,
    testRpcAsset,
    testAddress,
    'InBlock',
  ]);
  const [chain, egressId] = result.tx_details.response;

  assert.strictEqual(chain, testRpcAsset.chain, `Unexpected withdraw asset result`);
  assert(egressId > 0, `Unexpected egressId: ${egressId}`);

  await observeBalanceIncrease(testAsset, testAddress, oldBalance);
  console.log('=== testWithdrawAsset complete ===');
}

async function testRegisterWithExistingLpAccount() {
  console.log('=== Starting testWithdrawAsset ===');
  try {
    await lpApiRpc(`lp_register_account`, []);
    throw new Error(`Unexpected lp_register_account result`);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } catch (error: any) {
    // This account is already registered, so the command will fail.
    // This message is from the `AccountRoleAlreadyRegistered` pallet error.
    if (!error.message.includes('The account already has a registered role')) {
      throw new Error(`Unexpected lp_register_account error: ${error}`);
    }
  }
  console.log('=== testRegisterWithExistingLpAccount complete ===');
}

/// Test lp_set_range_order and lp_update_range_order by minting, updating, and burning a range order.

async function testRangeOrder() {
  console.log('=== Starting testRangeOrder ===');
  const range = { start: 1, end: 2 };
  const orderId = 74398; // Arbitrary order id so it does not interfere with other tests
  const zeroAssetAmounts = {
    AssetAmounts: {
      maximum: { base: 0, quote: 0 },
      minimum: { base: 0, quote: 0 },
    },
  };

  // Cleanup after any unfinished previous test so it does not interfere with this test
  await lpApiRpc(`lp_set_range_order`, [
    testRpcAsset,
    Assets.USDC,
    orderId,
    range,
    zeroAssetAmounts,
    'InBlock',
  ]);

  // Mint a range order
  const mintRangeOrder = (
    await lpApiRpc(`lp_set_range_order`, [
      testRpcAsset,
      Assets.USDC,
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
    await lpApiRpc(`lp_update_range_order`, [
      testRpcAsset,
      Assets.USDC,
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
    await lpApiRpc(`lp_set_range_order`, [
      testRpcAsset,
      Assets.USDC,
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

  console.log('=== testRangeOrder complete ===');
}

async function testGetOpenSwapChannels() {
  console.log('=== Starting testGetOpenSwapChannels ===');
  // TODO: Test with some SwapChannelInfo data
  const openSwapChannels = await lpApiRpc(`lp_get_open_swap_channels`, []);
  assert(openSwapChannels.ethereum, `Missing ethereum swap channel info`);
  assert(openSwapChannels.polkadot, `Missing polkadot swap channel info`);
  assert(openSwapChannels.bitcoin, `Missing bitcoin swap channel info`);
  console.log('=== testGetOpenSwapChannels complete ===');
}

/// Test lp_set_limit_order and lp_update_limit_order by minting, updating, and burning a limit order.

async function testLimitOrder() {
  console.log('=== Starting testLimitOrder ===');
  const orderId = 98432; // Arbitrary order id so it does not interfere with other tests
  const tick = 2;

  // Cleanup after any unfinished previous test so it does not interfere with this test
  await lpApiRpc(`lp_set_limit_order`, [testRpcAsset, Assets.USDC, 'sell', orderId, tick, 0]);

  // Mint a limit order
  const mintLimitOrder = (
    await lpApiRpc(`lp_set_limit_order`, [
      testRpcAsset,
      Assets.USDC,
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
    await lpApiRpc(`lp_update_limit_order`, [
      testRpcAsset,
      Assets.USDC,
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
    await lpApiRpc(`lp_set_limit_order`, [testRpcAsset, Assets.USDC, 'sell', orderId, tick, 0])
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

  console.log('=== testLimitOrder complete ===');
}

/// Runs all of the LP commands via the LP API Json RPC Server that is running and checks that the returned data is as expected
export async function testLpApi() {
  console.log('=== Starting LP API test ===');

  // Provide the amount of liquidity needed for the tests
  await provideLiquidityAndTestAssetBalances();

  await Promise.all([
    testRegisterLiquidityRefundAddress(),
    testLiquidityDeposit(),
    testWithdrawAsset(),
    testRegisterWithExistingLpAccount(),
    testRangeOrder(),
    testLimitOrder(),
    testGetOpenSwapChannels(),
  ]);

  console.log('=== LP API test complete ===');
}
