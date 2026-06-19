import { getChainflipPolkadotApi } from 'shared/utils/substrate';
import { createStateChainKeypair } from 'shared/utils';
import { type AssetAndChain } from '@chainflip/utils/chainflip';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { AccountRole, setupAccount } from 'shared/setup_account';

type AccountWithRole = {
  accountId: string;
  role: AccountRole;
};

async function setupKnownAccounts<A = []>(cf: ChainflipIO<A>): Promise<AccountWithRole[]> {
  await using chainflipApi = await getChainflipPolkadotApi();

  const operatorAccount = await setupAccount(cf, '//Operator_1', AccountRole.Operator);

  const currentAuthorities =
    (await chainflipApi.query.validator.currentAuthorities()) as unknown as string[];
  if (currentAuthorities.length === 0) {
    throw new Error('No validators found in currentAuthorities');
  }
  const validatorAccountId = currentAuthorities[0];

  return [
    { accountId: createStateChainKeypair('//LP_1').address, role: AccountRole.LiquidityProvider },
    { accountId: createStateChainKeypair('//BROKER_1').address, role: AccountRole.Broker },
    { accountId: validatorAccountId, role: AccountRole.Validator },
    { accountId: operatorAccount.address, role: AccountRole.Operator },
  ];
}

async function testRpcCallForAllAccounts<A = []>(
  cf: ChainflipIO<A>,
  rpcCallname: string,
  knownAccounts: AccountWithRole[],
) {
  await using chainflipApi = await getChainflipPolkadotApi();

  for (const account of knownAccounts) {
    try {
      cf.info(
        `Calling ${rpcCallname} for account ${account.accountId} with role ${AccountRole[account.role]}`,
      );
      const result = await chainflipApi.rpc(rpcCallname, account.accountId);
      cf.debug(
        `result of ${rpcCallname} for account ${account.accountId} with role ${AccountRole[account.role]} is : ${JSON.stringify(result)}`,
      );
    } catch (e) {
      throw new Error(
        `${rpcCallname} failed for ${AccountRole[account.role]} account ${account.accountId}: ${e}`,
      );
    }
  }
}

type SupportedAssets = {
  baseAsset: AssetAndChain;
  randomAsset: () => AssetAndChain;
};

async function getRuntimeSupportedAssets(): Promise<SupportedAssets> {
  await using chainflipApi = await getChainflipPolkadotApi();
  // Here we use cf_swapping_environment and parse the result instead of cf_supported_assets, because
  // cf_supported_assets implementation doesn't work at upgrade boundaries
  const env = (await chainflipApi.rpc('cf_swapping_environment')) as {
    network_fees: { regular_network_fee: { rates: Record<string, Record<string, number>> } };
  };
  const rates = env.network_fees.regular_network_fee.rates;
  const all = Object.entries(rates).flatMap(([chain, chainAssets]) =>
    Object.keys(chainAssets).map((asset) => ({ chain, asset })),
  );
  const assets = all.filter(
    (a) =>
      !(a.chain === 'Ethereum' && a.asset === 'USDC') &&
      a.chain !== 'Assethub' &&
      a.chain !== 'Polkadot' &&
      // Exclude Tron at upgrade boundaries: the old runtime doesn't have Tron
      // as a valid Asset variant, so calling pool RPCs with Tron panics it.
      // TODO: Remove after Tron is released.
      a.chain !== 'Tron',
  );
  return {
    baseAsset: { chain: 'Ethereum', asset: 'USDC' },
    randomAsset: () => assets[Math.floor(Math.random() * assets.length)] as AssetAndChain,
  };
}

async function testRpcCallForAssetPair<A = []>(
  cf: ChainflipIO<A>,
  rpcCallName: string,
  asset1: AssetAndChain,
  asset2: AssetAndChain,
) {
  await using chainflipApi = await getChainflipPolkadotApi();
  try {
    cf.info(
      `Calling ${rpcCallName} with asset1=${JSON.stringify(asset1)} asset2=${JSON.stringify(asset2)}`,
    );
    const result = await chainflipApi.rpc(rpcCallName, asset1, asset2);
    cf.debug(
      `result of ${rpcCallName}(${JSON.stringify(asset1)}, ${JSON.stringify(asset2)}): ${JSON.stringify(result)}`,
    );
  } catch (e) {
    throw new Error(
      `${rpcCallName}(${JSON.stringify(asset1)}, ${JSON.stringify(asset2)}) failed: ${e}`,
    );
  }
}

async function testParameterlessRpcCall<A = []>(cf: ChainflipIO<A>, rpcCallName: string) {
  await using chainflipApi = await getChainflipPolkadotApi();
  try {
    cf.info(`Calling ${rpcCallName}`);
    const result = await chainflipApi.rpc(rpcCallName);
    cf.debug(`result of ${rpcCallName}: ${JSON.stringify(result)}`);
  } catch (e) {
    throw new Error(`${rpcCallName} failed: ${e}`);
  }
}

async function printNodeAndRuntimeVersions<A = []>(cf: ChainflipIO<A>) {
  await using chainflipApi = await getChainflipPolkadotApi();
  const runtimeVersion = await chainflipApi.rpc('state_getRuntimeVersion');
  const nodeVersion = await chainflipApi.rpc('system_version');
  cf.info('-----------------------------------------------');
  cf.info(`Node version: ${JSON.stringify(nodeVersion)}`);
  cf.info(`Runtime spec version: ${(runtimeVersion as { specVersion: number }).specVersion}`);
  cf.info('-----------------------------------------------\n');
}

// Verify that custom RPC endpoints remain callable across runtime upgrades. When the runtime
// is upgraded, mismatches between what the custom rpc expects and the runtime API type encodings
// can cause runtime decode errors that are otherwise hard to catch until a user hits them in production.
export async function testRpcCalls(testContext: TestContext): Promise<void> {
  const cf = await newChainflipIO(testContext.logger, []);

  // Print node and runtime versions
  await printNodeAndRuntimeVersions(cf);

  // fetch supported assets before setting up accounts
  const assets = await getRuntimeSupportedAssets();

  // construct known accounts covering all possible account roles
  const knownAccounts = await setupKnownAccounts(cf);
  const lpAccounts = knownAccounts.filter((a) => a.role === AccountRole.LiquidityProvider);

  await cf.all([
    // Account based rpc calls
    (subcf) => testRpcCallForAllAccounts(subcf, 'cf_account_info', knownAccounts),
    (subcf) => testRpcCallForAllAccounts(subcf, 'cf_free_balances', knownAccounts),
    (subcf) => testRpcCallForAllAccounts(subcf, 'cf_lp_total_balances', lpAccounts),

    // Asset based rpc calls
    (subcf) =>
      testRpcCallForAssetPair(subcf, 'cf_pool_info', assets.randomAsset(), assets.baseAsset),
    (subcf) =>
      testRpcCallForAssetPair(subcf, 'cf_pool_liquidity', assets.randomAsset(), assets.baseAsset),
    (subcf) =>
      testRpcCallForAssetPair(subcf, 'cf_pool_orders', assets.randomAsset(), assets.baseAsset),
    (subcf) =>
      testRpcCallForAssetPair(subcf, 'cf_pool_price_v2', assets.randomAsset(), assets.baseAsset),
    (subcf) =>
      testRpcCallForAssetPair(subcf, 'cf_scheduled_swaps', assets.randomAsset(), assets.baseAsset),

    // read only rpc calls, often change
    (subcf) => testParameterlessRpcCall(subcf, 'cf_safe_mode_statuses'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_environment'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_funding_environment'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_swapping_environment'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_ingress_egress_environment'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_pools_environment'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_available_pools'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_get_trading_strategy_limits'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_lending_config'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_auction_parameters'),

    // read only rpc calls, mostly stable
    (subcf) => testParameterlessRpcCall(subcf, 'cf_all_account_infos'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_accounts'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_current_compatibility_version'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_lp_get_order_fills'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_supported_assets'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_boost_pools_depth'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_get_transaction_screening_events'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_controlled_vault_addresses'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_all_open_deposit_channels'),
    (subcf) => testParameterlessRpcCall(subcf, 'cf_get_vault_addresses'),
  ]);
}
