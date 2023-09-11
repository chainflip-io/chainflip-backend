#!/usr/bin/env -S pnpm tsx
import { assetDecimals } from '@chainflip-io/cli';
import assert from 'assert';
import {
  getChainflipApi,
  observeEvent,
  isValidHexHash,
  isValidEthAddress,
  amountToFineAmount,
  sleep,
} from './utils';
import { jsonRpc } from './json_rpc';
import { provideLiquidity } from './provide_liquidity';

const testEthAmount = 0.1;
const withdrawAssetAmount = parseInt(
  amountToFineAmount(testEthAmount.toString(), assetDecimals.ETH),
);
const testAssetAmount = withdrawAssetAmount;
const totalEthNeeded = testEthAmount * 5;
const chainflip = await getChainflipApi();
const ethAddress = '0x1594300cbd587694affd70c933b9ee9155b186d9';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function lpApiRpc(method: string, params: any[]): Promise<any> {
  // The port for the lp api is defined in `start_lp_api.sh`
  const port = 10589;
  return jsonRpc(method, params, port);
}

async function testAssetBalances() {
  const fineAmountNeeded = parseInt(
    amountToFineAmount(totalEthNeeded.toString(), assetDecimals.ETH),
  );

  // Wait for the balance to update
  let retryCount = 0;
  let ethBalance = 0;
  do {
    const balances = await lpApiRpc(`lp_assetBalances`, []);
    ethBalance = balances.Eth;
    retryCount++;
    if (retryCount > 120) {
      throw new Error(
        `Not enough Eth for test (${fineAmountNeeded}). balances: ${JSON.stringify(balances)}`,
      );
    }
    await sleep(1000);
  } while (ethBalance < fineAmountNeeded);
}

async function testRegisterEmergencyWithdrawalAddress() {
  const observeRegisterEwaEvent = observeEvent(
    'liquidityProvider:EmergencyWithdrawalAddressRegistered',
    chainflip,
    (event) => event.data.address.Eth === ethAddress,
  );

  const registerEwa = await lpApiRpc(`lp_registerEmergencyWithdrawalAddress`, [
    'Ethereum',
    ethAddress,
  ]);
  if (!isValidHexHash(await registerEwa)) {
    throw new Error(`Unexpected lp_registerEmergencyWithdrawalAddress result`);
  }
  await observeRegisterEwaEvent;
}

async function testLiquidityDeposit() {
  const observeLiquidityDepositEvent = observeEvent(
    'liquidityProvider:LiquidityDepositAddressReady',
    chainflip,
    (event) => event.data.depositAddress.Eth,
  );
  const liquidityDepositResult = await lpApiRpc(`lp_liquidityDeposit`, ['Eth']);
  const liquidityDepositEvent = await observeLiquidityDepositEvent;

  assert.strictEqual(
    liquidityDepositEvent.data.depositAddress.Eth,
    liquidityDepositResult,
    `Incorrect deposit address`,
  );
  assert(
    isValidEthAddress(liquidityDepositResult),
    `Invalid deposit address: ${liquidityDepositResult}`,
  );
}

async function testWithdrawAsset() {
  const withdrawAsset = await lpApiRpc(`lp_withdrawAsset`, [
    withdrawAssetAmount,
    'Eth',
    ethAddress,
  ]);
  assert.strictEqual(withdrawAsset[0], 'Ethereum', `Unexpected withdraw asset result`);
  const egressId = withdrawAsset[1];
  assert(egressId > 0, `Unexpected egressId: ${egressId}`);
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
  const orderId = 1;
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
    `Unexpected mint range order result`,
  );
  assert(mintRangeOrder[0].liquidity_total > 0, `Unexpected mint range order result`);

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
    `Unexpected update range order result`,
  );
  assert(updateRangeOrder[0].liquidity_total > 0, `Unexpected update range order result`);

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
    `Unexpected burn range order result`,
  );
  assert.strictEqual(burnRangeOrder[0].liquidity_total, 0, `Unexpected burn range order result`);
}

/// Test lp_setLimitOrder and lp_updateLimitOrder by minting, updating, and burning a limit order.
async function testLimitOrder() {
  const orderId = 2;
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
    `Unexpected mint limit order result`,
  );
  assert.strictEqual(
    mintLimitOrder[0].amount_total,
    testAssetAmount,
    `Unexpected mint limit order result`,
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
    `Unexpected update limit order result`,
  );
  assert.strictEqual(
    updateLimitOrder[0].amount_total,
    testAssetAmount * 2,
    `Unexpected update limit order result`,
  );

  // Burn the limit order
  const burnLimitOrder = await lpApiRpc(`lp_setLimitOrder`, ['Eth', 'Usdc', orderId, tick, 0]);
  assert(burnLimitOrder.length >= 1, `Empty burn limit order result`);
  assert.strictEqual(
    burnLimitOrder[0].increase_or_decrease,
    `Decrease`,
    `Unexpected burn limit order result`,
  );
  assert.strictEqual(burnLimitOrder[0].amount_total, 0, `Unexpected burn limit order result`);
}

/// Runs all of the LP commands via the LP API Json RPC Server that is running and checks that the returned data is as expected
export async function testLpApi() {
  // We have to wait finalization here because the LP API server is using a finalized block stream
  await provideLiquidity('ETH', totalEthNeeded, true);

  // Check that we have enough eth to do the rest of the tests
  await testAssetBalances();

  await Promise.all([
    testRegisterEmergencyWithdrawalAddress(),
    testLiquidityDeposit(),
    testWithdrawAsset(),
    testRegisterWithExistingLpAccount(),
    testRangeOrder(),
    testLimitOrder(),
  ]);
}
