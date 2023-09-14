import { assetDecimals } from '@chainflip-io/cli';
import assert from 'assert';
import {
  getChainflipApi,
  observeEvent,
  isValidHexHash,
  isValidEthAddress,
  amountToFineAmount,
  sleep,
  observeBalanceIncrease,
} from './utils';
import { jsonRpc } from './json_rpc';
import { provideLiquidity } from './provide_liquidity';
import { sendEth } from './send_eth';
import { getBalance } from './get_balance';

const testEthAmount = 0.1;
const testAssetAmount = parseInt(amountToFineAmount(testEthAmount.toString(), assetDecimals.ETH));
const ethToProvide = testEthAmount * 50; // Provide plenty of eth for the tests
const chainflip = await getChainflipApi();
const ethAddress = '0x1594300cbd587694affd70c933b9ee9155b186d9';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function lpApiRpc(method: string, params: any[]): Promise<any> {
  // The port for the lp api is defined in `start_lp_api.sh`
  const port = 10589;
  return jsonRpc(method, params, port);
}

async function provideLiquidityAndTestAssetBalances() {
  const fineAmountToProvide = parseInt(
    amountToFineAmount(ethToProvide.toString(), assetDecimals.ETH),
  );

  // We have to wait finalization here because the LP API server is using a finalized block stream (This may change in PRO-777 PR#3986)
  await provideLiquidity('ETH', ethToProvide, true);

  // Wait for the LP API to get the balance update, just incase it was slower than us to see the event.
  let retryCount = 0;
  let ethBalance = 0;
  do {
    const balances = await lpApiRpc(`lp_assetBalances`, []);
    ethBalance = balances.Eth;
    retryCount++;
    if (retryCount > 14) {
      throw new Error(
        `Failed to provide eth for tests (${fineAmountToProvide}). balances: ${JSON.stringify(
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
    (event) => event.data.address.Eth === ethAddress,
  );

  const registerRefundAddress = await lpApiRpc(`lp_registerLiquidityRefundAddress`, [
    'Ethereum',
    ethAddress,
  ]);
  if (!isValidHexHash(await registerRefundAddress)) {
    throw new Error(`Unexpected lp_registerLiquidityRefundAddress result`);
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
  // TODO: This result will need to be updated after #3995 is merged
  const liquidityDepositAddress = await lpApiRpc(`lp_liquidityDeposit`, ['Eth']);
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
      event.data.asset.toUpperCase() === 'ETH' &&
      Number(event.data.amountCredited.replace(/,/g, '')) === testAssetAmount,
  );
  await sendEth(liquidityDepositAddress, String(testEthAmount));
  await observeAccountCreditedEvent;
}

async function testWithdrawAsset() {
  const oldBalance = await getBalance('ETH', ethAddress);

  const [asset, egressId] = await lpApiRpc(`lp_withdrawAsset`, [
    testAssetAmount,
    'Eth',
    ethAddress,
  ]);
  assert.strictEqual(asset, 'Ethereum', `Unexpected withdraw asset result`);
  assert(egressId > 0, `Unexpected egressId: ${egressId}`);

  await observeBalanceIncrease('ETH', ethAddress, oldBalance);
}

async function testRegisterWithExistingLpAccount() {
  try {
    await lpApiRpc(`lp_registerAccount`, []);
    throw new Error(`Unexpected lp_registerAccount result`);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } catch (error: any) {
    // This account is already registered, so the command will fail.
    if (!error.message.includes('Could not register account role for account')) {
      throw new Error(`Unexpected lp_registerAccount error: ${JSON.stringify(error)}`);
    }
  }
}

/// Test lp_setRangeOrder and lp_updateRangeOrder by minting, updating, and burning a range order.
async function testRangeOrder() {
  const range = { start: 1, end: 2 };
  const orderId = 74398; // Arbitrary order id so it does not interfere with other tests
  const zeroAssetAmounts = {
    AssetAmounts: {
      maximum: { base: 0, pair: 0 },
      minimum: { base: 0, pair: 0 },
    },
  };

  // Cleanup after any unfinished previous test so it does not interfere with this test
  await lpApiRpc(`lp_setRangeOrder`, ['Usdc', 'Eth', orderId, range, zeroAssetAmounts]);

  // Mint a range order
  const mintRangeOrder = await lpApiRpc(`lp_setRangeOrder`, [
    'Usdc',
    'Eth',
    orderId,
    range,
    {
      AssetAmounts: {
        maximum: { base: 0, pair: testAssetAmount },
        minimum: { base: 0, pair: 0 },
      },
    },
  ]);

  assert(mintRangeOrder.length >= 1, `Empty mint range order result`);
  assert.strictEqual(
    mintRangeOrder[0].increase_or_decrease,
    `Increase`,
    `Expected mint of range order to increase liquidity`,
  );
  assert(
    mintRangeOrder[0].liquidity_total > 0,
    `Expected range order to have liquidity after mint`,
  );

  // Update the range order
  const updateRangeOrder = await lpApiRpc(`lp_updateRangeOrder`, [
    'Usdc',
    'Eth',
    orderId,
    range,
    `Increase`,
    {
      AssetAmounts: {
        maximum: { base: 0, pair: testAssetAmount },
        minimum: { base: 0, pair: 0 },
      },
    },
  ]);

  assert(updateRangeOrder.length >= 1, `Empty update range order result`);
  assert.strictEqual(
    updateRangeOrder[0].increase_or_decrease,
    `Increase`,
    `Expected positive update of range order to increase liquidity`,
  );
  assert(
    updateRangeOrder[0].liquidity_total > 0,
    `Expected range order to have liquidity after update`,
  );

  // Burn the range order
  const burnRangeOrder = await lpApiRpc(`lp_setRangeOrder`, [
    'Usdc',
    'Eth',
    orderId,
    range,
    zeroAssetAmounts,
  ]);

  assert(burnRangeOrder.length >= 1, `Empty burn range order result`);
  assert.strictEqual(
    burnRangeOrder[0].increase_or_decrease,
    `Decrease`,
    `Expected burn range order to decrease liquidity`,
  );
  assert.strictEqual(
    burnRangeOrder[0].liquidity_total,
    0,
    `Expected burn range order to result in 0 liquidity total`,
  );
}

async function testGetOpenSwapChannels() {
  // TODO: Test with some SwapChannelInfo data
  const openSwapChannels = await lpApiRpc(`lp_getOpenSwapChannels`, []);
  assert(openSwapChannels.ethereum, `Missing ethereum swap channel info`);
  assert(openSwapChannels.polkadot, `Missing polkadot swap channel info`);
  assert(openSwapChannels.bitcoin, `Missing bitcoin swap channel info`);
}

/// Test lp_setLimitOrder and lp_updateLimitOrder by minting, updating, and burning a limit order.
async function testLimitOrder() {
  const orderId = 98432; // Arbitrary order id so it does not interfere with other tests
  const tick = 2;

  // Cleanup after any unfinished previous test so it does not interfere with this test
  await lpApiRpc(`lp_setLimitOrder`, ['Eth', 'Usdc', orderId, tick, 0]);

  // Mint a limit order
  const mintLimitOrder = await lpApiRpc(`lp_setLimitOrder`, [
    'Eth',
    'Usdc',
    orderId,
    tick,
    testAssetAmount,
  ]);
  assert(mintLimitOrder.length >= 1, `Empty mint limit order result`);
  assert.strictEqual(
    mintLimitOrder[0].increase_or_decrease,
    `Increase`,
    `Expected mint of limit order to increase liquidity`,
  );
  assert.strictEqual(
    mintLimitOrder[0].amount_total,
    testAssetAmount,
    `Unexpected amount of asset was minted for limit order`,
  );

  // Update the limit order
  const updateLimitOrder = await lpApiRpc(`lp_updateLimitOrder`, [
    'Eth',
    'Usdc',
    orderId,
    tick,
    `Increase`,
    testAssetAmount,
  ]);
  assert(updateLimitOrder.length >= 1, `Empty update limit order result`);
  assert.strictEqual(
    updateLimitOrder[0].increase_or_decrease,
    `Increase`,
    `Expected positive update of limit order to increase liquidity`,
  );
  assert.strictEqual(
    updateLimitOrder[0].amount_total,
    testAssetAmount * 2,
    `Unexpected amount of asset was minted after updating limit order`,
  );

  // Burn the limit order
  const burnLimitOrder = await lpApiRpc(`lp_setLimitOrder`, ['Eth', 'Usdc', orderId, tick, 0]);
  assert(burnLimitOrder.length >= 1, `Empty burn limit order result`);
  assert.strictEqual(
    burnLimitOrder[0].increase_or_decrease,
    `Decrease`,
    `Expected burn limit order to decrease liquidity`,
  );
  assert.strictEqual(
    burnLimitOrder[0].amount_total,
    0,
    `Expected burn limit order to result in 0 amount total`,
  );
}

/// Runs all of the LP commands via the LP API Json RPC Server that is running and checks that the returned data is as expected
export async function testLpApi() {
  // Provide the amount of eth needed for the tests
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
}
