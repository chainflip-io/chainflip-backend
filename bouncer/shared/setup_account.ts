import { getChainflipApi } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
// eslint-disable-next-line no-restricted-imports
import { KeyringPair } from '@polkadot/keyring/types';
import { ChainflipIO, partialAccountFromUri } from 'shared/utils/chainflip_io';
import { accountRolesAccountRoleRegistered } from 'generated/events/accountRoles/accountRoleRegistered';

export enum AccountRole {
  Unregistered,
  LiquidityProvider,
  Broker,
  Operator,
}

async function getAccountRole(address: string): Promise<AccountRole> {
  await using chainflip = await getChainflipApi();

  const role = JSON.stringify(await chainflip.query.accountRoles.accountRoles(address)).replace(
    /"/g,
    '',
  );

  switch (role) {
    case 'null':
    case 'Unregistered':
      return AccountRole.Unregistered;
    case 'LiquidityProvider':
      return AccountRole.LiquidityProvider;
    case 'Broker':
      return AccountRole.Broker;
    case 'Operator':
      return AccountRole.Operator;
    default:
      throw new Error(`Unknown account role: ${role}`);
  }
}

/**
 * Checks if the account is already registered as the given role and if not, registers it and funds it with some Flip.
 * Errors if the account is already registered with a different role.
 * @param parentCf the parent chainflip IO instance
 * @param uri The URI of the account to set up. eg '//LP_1'
 * @param accountRole The role to register the account as. eg AccountRole.LiquidityProvider
 * @param flipFundAmount (optional) The amount of Flip to fund the account with. Default is '1000'.
 * @return The KeyringPair of the set up account.
 */
export async function setupAccount<A = []>(
  parentCf: ChainflipIO<A>,
  uri: `//${string}`,
  accountRole: AccountRole,
  flipFundAmount = '1000',
): Promise<KeyringPair> {
  const cf = parentCf.with({ account: partialAccountFromUri(uri) });

  const account = cf.requirements.account.keypair;
  const accountUri = cf.requirements.account.uri;

  // Check for existing role
  cf.trace(`Checking existing role for ${uri}`);
  const role = await getAccountRole(account.address);

  if (role === accountRole) {
    cf.debug(`${account.address} is already registered as an ${AccountRole[accountRole]}`);
    return account;
  }
  if (role !== AccountRole.Unregistered) {
    throw new Error(
      `Cannot register ${uri} as ${AccountRole[accountRole]} because it has a role: ${role}`,
    );
  }

  // Fund the account
  await fundFlip(cf, account.address, flipFundAmount);

  // Register account
  cf.debug(`Registering ${account.address} as an ${AccountRole[accountRole]}...`);

  const accountRoleRegisteredEvent = await cf.submitExtrinsic({
    extrinsic: (api) => {
      switch (accountRole) {
        case AccountRole.LiquidityProvider:
          return api.tx.liquidityProvider.registerLpAccount();
        case AccountRole.Broker:
          return api.tx.swapping.registerAsBroker();
        case AccountRole.Operator:
          return api.tx.validator.registerAsOperator(
            {
              feeBps: 2_000,
              delegationAcceptance: 'Allow',
            },
            accountUri,
          );
        default:
          throw new Error(`Unsupported registration as account role: ${accountRole}`);
      }
    },
    expectedEvent: {
      name: 'AccountRoles.AccountRoleRegistered',
      schema: accountRolesAccountRoleRegistered.refine(
        (event) => event.accountId === account.address,
      ),
    },
  });

  cf.debug(
    `${accountRoleRegisteredEvent.accountId} successfully registered as an ${AccountRole[accountRole]}}`,
  );

  return account;
}
