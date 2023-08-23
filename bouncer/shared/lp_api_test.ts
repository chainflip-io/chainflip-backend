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

interface RangeOrder {
  lower_tick: number;
  upper_tick: number;
  liquidity: string;
}

const testEthAmount = 0.1;
const withdrawAssetAmount = parseInt(
  amountToFineAmount(testEthAmount.toString(), assetDecimals.ETH),
);
const mintAssetAmount = withdrawAssetAmount;
const totalEthNeeded = testEthAmount * 3;
const chainflip = await getChainflipApi();
const ethAddress = '0x1594300cbd587694affd70c933b9ee9155b186d9';

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function lpApiRpc(method: string, params: any[]): Promise<any> {
  // The port for the lp api is defined in `start_lp_api.sh`
  const port = 10589;
  return jsonRpc(method, params, port);
}

async function testGetPools() {
  // Check that the pool is ready for the test and test the getPools commands
  const pools = await lpApiRpc(`lp_getPools`, []);
  if (!pools.Eth) {
    throw new Error(`Eth pool does not exists, has the setup been run?`);
  }
  assert.strictEqual(pools.Eth.enabled, true, `Eth pool is not enabled`);

  const ethPool = await lpApiRpc(`lp_getPool`, ['Eth']);
  assert.strictEqual(
    JSON.stringify(ethPool),
    JSON.stringify(pools.Eth),
    `Mismatch pool data returned`,
  );
}

async function testAssetBalances() {
  let balances = await lpApiRpc(`lp_assetBalances`, []);
  const fineAmountNeeded = parseInt(
    amountToFineAmount(totalEthNeeded.toString(), assetDecimals.ETH),
  );

  // Wait for the balance to update
  let retryCount = 0;
  while (balances.Eth < fineAmountNeeded) {
    retryCount++;
    if (retryCount > 10) {
      throw new Error(
        `Not enough Eth for test (${fineAmountNeeded}). balances: ${JSON.stringify(balances)}`,
      );
    }
    await sleep(1000);
    balances = await lpApiRpc(`lp_assetBalances`, []);
  }
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

async function testRegisterAccount() {
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

async function testRangeOrder() {
  const lowerTick = 1;
  const upperTick = 2;

  // Cleanup after any unfinished previous test so it does not interfere with this test
  const existingRangeOrders = await lpApiRpc(`lp_getRangeOrders`, []);
  const existingTestRangeOrder = existingRangeOrders.Eth.find(
    (rangeOrder: RangeOrder) =>
      rangeOrder.lower_tick === lowerTick && rangeOrder.upper_tick === upperTick,
  );
  if (existingTestRangeOrder) {
    console.log('Found existing test range order, burning it');
    await lpApiRpc(`lp_burnRangeOrder`, [
      'Eth',
      lowerTick,
      upperTick,
      existingTestRangeOrder.liquidity,
    ]);
  }

  // Mint a range order
  const mintRangeOrder = await lpApiRpc(`lp_mintRangeOrder`, [
    'Eth',
    lowerTick,
    upperTick,
    {
      AssetAmounts: {
        desired: { unstable: mintAssetAmount, stable: 0 },
        minimum: { unstable: 0, stable: 0 },
      },
    },
  ]);

  assert.strictEqual(
    mintRangeOrder.assets_debited.zero,
    mintAssetAmount,
    `Unexpected mint range order result`,
  );

  // Check that the range order was minted
  const rangeOrders = await lpApiRpc(`lp_getRangeOrders`, []);
  const rangeOrder = rangeOrders.Eth.find(
    (i: RangeOrder) => i.lower_tick === lowerTick && i.upper_tick === upperTick,
  );
  if (!rangeOrder) {
    throw new Error(`Did not find minted range order ${JSON.stringify(rangeOrders.Eth)}`);
  }

  // Burn the range order
  const burnRangeOrder = await lpApiRpc(`lp_burnRangeOrder`, [
    'Eth',
    lowerTick,
    upperTick,
    rangeOrder.liquidity,
  ]);
  assert.strictEqual(
    burnRangeOrder.assets_credited.zero,
    mintAssetAmount,
    `Unexpected burn range order result`,
  );

  // Check that the range order is gone
  const rangeOrdersAfterBurn = await lpApiRpc(`lp_getRangeOrders`, []);
  if (
    rangeOrdersAfterBurn.Eth.find(
      (i: RangeOrder) => i.lower_tick === lowerTick && i.upper_tick === upperTick,
    )
  ) {
    throw new Error(`Range order was not burnt ${JSON.stringify(rangeOrders.Eth)}`);
  }
}

async function testLimitOrder() {
  const price = 2;
  const mintLimitOrder = await lpApiRpc(`lp_mintLimitOrder`, [
    'Eth',
    'Sell',
    price,
    mintAssetAmount,
  ]);

  assert.strictEqual(
    mintLimitOrder.assets_debited,
    mintAssetAmount,
    `Unexpected mint limit order result`,
  );

  const burnLimitOrder = await lpApiRpc(`lp_burnLimitOrder`, [
    'Eth',
    'Sell',
    price,
    mintAssetAmount,
  ]);
  assert.strictEqual(
    burnLimitOrder.assets_credited,
    mintAssetAmount,
    `Unexpected burn limit order result`,
  );
}

/// Runs all of the LP commands via the LP API Json RPC Server that is running and checks that the returned data is as expected
export async function testLpApi() {
  // Confirm that the pools are ready to be used by the other tests
  await testGetPools();

  // We have to wait finalization here because the LP API server is using a finalized block stream
  await provideLiquidity('ETH', totalEthNeeded, true);

  // Check that we have enough eth to do the rest of the tests
  await testAssetBalances();

  await Promise.all([
    testRegisterEmergencyWithdrawalAddress(),
    testLiquidityDeposit(),
    testWithdrawAsset(),
    testRegisterAccount(),
    testRangeOrder(),
    testLimitOrder(),
  ]);
}
