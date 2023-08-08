#!/usr/bin/env -S pnpm tsx
import assert from 'assert';
import { getChainflipApi, observeEvent, isValidHexHash, isValidEthAddress } from './utils';
import { jsonRpc } from './json_rpc';

interface RangeOrder {
  lower_tick: number;
  upper_tick: number;
  liquidity: string;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
async function lpApiRpc(method: string, params: any[], throwError = true): Promise<any> {
  // The port for the lp api is defined in `start_lp_api.sh`
  const port = 10589;
  return jsonRpc(method, params, port, throwError);
}

/// Runs all of the LP commands via the LP API Json RPC Server that is running and checks that the returned data is as expected
export async function testLpApi() {
  const chainflip = await getChainflipApi();

  // Check that the pool is ready for the test and test the getPools commands
  const pools = await lpApiRpc(`lp_getPools`, []);
  if (!pools.Eth) {
    throw new Error(`Eth pool does not exists, has the setup been run?`);
  }
  assert.strictEqual(pools.Eth.enabled, true, `Eth pool is not enabled`);

  const ethPool = await lpApiRpc(`lp_getPool`, ['Eth']);
  assert.strictEqual(ethPool.Eth.enabled, pools.Eth.enabled, `Mismatch pool data returned`);
  assert.strictEqual(
    ethPool.Eth.pool_state.limit_orders.positions.length,
    pools.Eth.pool_state.limit_orders.positions.length,
    `Mismatch pool data returned`,
  );
  assert.strictEqual(
    ethPool.Eth.pool_state.range_orders.positions.length,
    pools.Eth.pool_state.range_orders.positions.length,
    `Mismatch pool data returned`,
  );

  // Check that the account has the required Eth and test the assetBalances command
  const balances = await lpApiRpc(`lp_assetBalances`, []);
  const withdrawAmount = 1;
  const mintAmount = 1000000;
  // TODO: Calculate the required amount of Eth for the mint commands
  if (balances.Eth < withdrawAmount) {
    throw new Error(
      `Need at least ${withdrawAmount} Eth for the test to work, has the setup been run?`,
    );
  }

  // Check the range order doesn't already exist
  // TODO: What range should we use?
  const lowerTick = 1;
  const upperTick = 2;
  const ExistingRangeOrders = await lpApiRpc(`lp_getRangeOrders`, []);
  const existingTestRangeOrder = ExistingRangeOrders.Eth.find(
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

  // Observe some events for comparison
  const ethAddress = '0x1594300cbd587694affd70c933b9ee9155b186d9';
  const observeRegisterEwaEvent = observeEvent(
    'liquidityProvider:EmergencyWithdrawalAddressRegistered',
    chainflip,
    (event) => event.data.address.Eth === ethAddress,
  );
  const observeLiquidityDepositEvent = observeEvent(
    'liquidityProvider:LiquidityDepositAddressReady',
    chainflip,
  );

  // Test some misc commands
  const registerEwa = lpApiRpc(`lp_registerEmergencyWithdrawalAddress`, ['Ethereum', ethAddress]);
  const liquidityDeposit = lpApiRpc(`lp_liquidityDeposit`, ['Eth']);
  const withdrawAsset = lpApiRpc(`lp_withdrawAsset`, [withdrawAmount, 'Eth', ethAddress]);
  const registerAccount = lpApiRpc(`lp_registerAccount`, [], false);
  // Test the mint commands
  const mintRangeOrder = lpApiRpc(`lp_mintRangeOrder`, [
    'Eth',
    lowerTick,
    upperTick,
    { PoolLiquidity: mintAmount },
  ]);
  const mintLimitOrder = lpApiRpc(`lp_mintLimitOrder`, ['Eth', 'Sell', upperTick, mintAmount]);

  if (!isValidHexHash(await registerEwa)) {
    throw new Error(`Unexpected lp_registerEmergencyWithdrawalAddress result`);
  }
  await observeRegisterEwaEvent;

  const liquidityDepositEvent = await observeLiquidityDepositEvent;
  const liquidityDepositResult = await liquidityDeposit;
  assert.strictEqual(
    liquidityDepositEvent.data.depositAddress.Eth,
    liquidityDepositResult,
    `Incorrect deposit address`,
  );
  assert(
    isValidEthAddress(liquidityDepositResult),
    `Invalid deposit address: ${liquidityDepositResult}`,
  );

  const withdrawAssetResult = await withdrawAsset;
  // TODO: What value to expect?
  assert.strictEqual(withdrawAssetResult[0], 'Ethereum', `Unexpected withdraw result`);
  assert(withdrawAssetResult[1] > 0, `Unexpected withdraw result ${withdrawAssetResult[1]}`);

  assert.strictEqual(
    (await registerAccount).message,
    'Could not register account role for account',
    `Unexpected register account result`,
  );

  // TODO: What value to expect?
  assert((await mintRangeOrder).assets_debited.zero > 0, `Unexpected mint range order result`);

  assert.strictEqual(
    (await mintLimitOrder).assets_debited,
    mintAmount,
    `Unexpected mint limit order result`,
  );

  // Check that the range order was minted
  const rangeOrders = await lpApiRpc(`lp_getRangeOrders`, []);
  assert(
    rangeOrders.Eth.find(
      (rangeOrder: RangeOrder) =>
        rangeOrder.lower_tick === lowerTick && rangeOrder.upper_tick === upperTick,
    ),
    `Did not find minted range order ${JSON.stringify(rangeOrders.Eth)}`,
  );

  // Test the burn commands
  const burnRangeOrder = lpApiRpc(`lp_burnRangeOrder`, ['Eth', lowerTick, upperTick, mintAmount]);
  const burnLimitOrder = lpApiRpc(`lp_burnLimitOrder`, ['Eth', 'Sell', upperTick, mintAmount]);

  // TODO: What value to expect?
  assert((await burnRangeOrder).assets_credited.zero > 0, `Unexpected burn range order result`);

  assert.strictEqual(
    (await burnLimitOrder).assets_credited,
    mintAmount,
    `Unexpected burn limit order result`,
  );
}
