import { getChainflipApi } from 'shared/utils/substrate';
import { createStateChainKeypair } from 'shared/utils';
import { TestContext } from 'shared/utils/test_context';
import { ChainflipIO, newChainflipIO } from 'shared/utils/chainflip_io';
import { AccountRole, setupAccount } from 'shared/setup_account';

type AccountWithRole = {
  accountId: string;
  role: AccountRole;
};

async function setupKnownAccounts<A = []>(cf: ChainflipIO<A>): Promise<AccountWithRole[]> {
  await using chainflipApi = await getChainflipApi();

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
  await using chainflipApi = await getChainflipApi();

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

async function testParameterlessRpcCall<A = []>(cf: ChainflipIO<A>, rpcCallName: string) {
  await using chainflipApi = await getChainflipApi();
  try {
    cf.info(`Calling ${rpcCallName}`);
    const result = await chainflipApi.rpc(rpcCallName);
    cf.debug(`result of ${rpcCallName}: ${JSON.stringify(result)}`);
  } catch (e) {
    throw new Error(`${rpcCallName} failed: ${e}`);
  }
}

// Verify that custom RPC endpoints remain callable across runtime upgrades. When the runtime
// is upgraded, mismatches between what the custom rpc expects and the runtime API type encodings
// can cause runtime decode errors that are otherwise hard to catch until a user hits them in production.
export async function testRpcCalls(testContext: TestContext): Promise<void> {
  const cf = await newChainflipIO(testContext.logger, []);

  // construct known accounts covering all possible account roles
  const knownAccounts = await setupKnownAccounts(cf);
  const lpAccounts = knownAccounts.filter((a) => a.role === AccountRole.LiquidityProvider);

  await cf.all([
    (subcf) => testRpcCallForAllAccounts(subcf, 'cf_account_info_v2', knownAccounts),
    (subcf) => testRpcCallForAllAccounts(subcf, 'cf_free_balances', knownAccounts),
    (subcf) => testRpcCallForAllAccounts(subcf, 'cf_lp_total_balances', lpAccounts),

    (subcf) => testParameterlessRpcCall(subcf, 'cf_environment'),
  ]);
}
