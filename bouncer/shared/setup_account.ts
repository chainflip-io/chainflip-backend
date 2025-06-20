import assert from 'assert';
import { lpMutex, handleSubstrateError, createStateChainKeypair } from 'shared/utils';
import { getChainflipApi, observeEvent } from 'shared/utils/substrate';
import { fundFlip } from 'shared/fund_flip';
import { Logger } from 'shared/utils/logger';

export async function setupLpAccount(logger: Logger, uri: string) {
  const lp = createStateChainKeypair(uri);

  await fundFlip(logger, lp.address, '1000');
  logger.debug(`Registering ${lp.address} as an LP...`);

  await using chainflip = await getChainflipApi();

  const eventHandle = observeEvent(logger, 'accountRoles:AccountRoleRegistered', {
    test: (event) => event.data.accountId === lp.address,
  }).event;

  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .registerLpAccount()
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await eventHandle;

  logger.debug(`${lp.address} successfully registered as an LP`);
}

/// Sets up a broker account by registering it as a broker if it is not already registered and funding it with 1000 Flip.
export async function setupBrokerAccount(logger: Logger, uri: string) {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair(uri);

  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(broker.address),
  ).replace(/"/g, '');

  if (role === 'null' || role === 'Unregistered') {
    await fundFlip(logger, broker.address, '1000');
    logger.debug(`Registering ${broker.address} as a Broker...`);

    const eventHandle = observeEvent(logger, 'accountRoles:AccountRoleRegistered', {
      test: (event) => event.data.accountId === broker.address,
    }).event;

    await lpMutex.runExclusive(async () => {
      await chainflip.tx.swapping
        .registerAsBroker()
        .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
    });
    await eventHandle;

    logger.debug(`${broker.address} successfully registered as a Broker`);
  } else {
    assert.strictEqual(
      role,
      'Broker',
      `Cannot register ${uri} as broker because it has a role: ${role}`,
    );
  }
}
