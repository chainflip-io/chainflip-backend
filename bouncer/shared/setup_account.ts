import { cfMutex, handleSubstrateError, createStateChainKeypair } from 'shared/utils';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
import { Logger } from 'shared/utils/logger';
// eslint-disable-next-line no-restricted-imports
import { KeyringPair } from '@polkadot/keyring/types';

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
 * @param uri The URI of the account to set up. eg '//LP_1'
 * @param accountRole The role to register the account as. eg AccountRole.LiquidityProvider
 * @param flipFundAmount (optional) The amount of Flip to fund the account with. Default is '1000'.
 * @return The KeyringPair of the set up account.
 */
export async function setupAccount(
  logger: Logger,
  uri: string,
  accountRole: AccountRole,
  flipFundAmount = '1000',
): Promise<KeyringPair> {
  const account = createStateChainKeypair(uri);

  await cfMutex.runExclusive(uri, async () => {
    await using chainflip = await getChainflipApi();

    // Check for existing role
    logger.trace(`Checking existing role for ${uri}`);
    const role = await getAccountRole(account.address);

    if (role === accountRole) {
      logger.debug(`${account.address} is already registered as an ${AccountRole[accountRole]}`);
      return;
    }
    if (role !== AccountRole.Unregistered) {
      throw new Error(
        `Cannot register ${uri} as ${AccountRole[accountRole]} because it has a role: ${role}`,
      );
    }

    // Fund the account
    await fundFlip(logger, account.address, flipFundAmount);

    // Register account
    logger.debug(`Registering ${account.address} as an ${AccountRole[accountRole]}...`);
    const eventHandle = observeEvent(logger, 'accountRoles:AccountRoleRegistered', {
      test: (event) => event.data.accountId === account.address,
      timeoutSeconds: 30,
    }).event;

    const nonce = await chainflip.rpc.system.accountNextIndex(account.address);
    let extrinsic;
    switch (accountRole) {
      case AccountRole.LiquidityProvider:
        extrinsic = chainflip.tx.liquidityProvider.registerLpAccount();
        break;
      case AccountRole.Broker:
        extrinsic = chainflip.tx.swapping.registerAsBroker();
        break;
      case AccountRole.Operator:
        extrinsic = chainflip.tx.validator.registerAsOperator(
          {
            feeBps: 2_000,
            delegationAcceptance: 'Allow',
          },
          uri,
        );
        break;
      default:
        throw new Error(`Unsupported registration as account role: ${accountRole}`);
    }

    await extrinsic.signAndSend(account, { nonce }, handleSubstrateError(chainflip));

    await eventHandle;
    logger.debug(`${account.address} successfully registered as an ${AccountRole[accountRole]}`);
  });

  return account;
}
