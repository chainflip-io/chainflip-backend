import assert from 'assert';
import { lpMutex, handleSubstrateError, createStateChainKeypair } from './utils';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { fundFlip } from './fund_flip';

export async function setupLpAccount(uri: string) {
  const lp = createStateChainKeypair(uri);

  await fundFlip(lp.address, '1000');
  console.log(`Registering ${lp.address} as an LP...`);

  await using chainflip = await getChainflipApi();

  const eventHandle = observeEvent('accountRoles:AccountRoleRegistered', {
    test: (event) => event.data.accountId === lp.address,
  }).event;

  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .registerLpAccount()
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  await eventHandle;

  console.log(`${lp.address} successfully registered as an LP`);
}

/// Sets up a broker account by registering it as a broker if it is not already registered and funding it with 1000 Flip.
export async function setupBrokerAccount(uri: string) {
  await using chainflip = await getChainflipApi();

  const broker = createStateChainKeypair(uri);

  const role = JSON.stringify(
    await chainflip.query.accountRoles.accountRoles(broker.address),
  ).replace(/"/g, '');

  if (role === 'null' || role === 'Unregistered') {
    await fundFlip(broker.address, '1000');
    console.log(`Registering ${broker.address} as a Broker...`);

    const eventHandle = observeEvent('accountRoles:AccountRoleRegistered', {
      test: (event) => event.data.accountId === broker.address,
    }).event;

    await lpMutex.runExclusive(async () => {
      await chainflip.tx.swapping
        .registerAsBroker()
        .signAndSend(broker, { nonce: -1 }, handleSubstrateError(chainflip));
    });
    await eventHandle;

    console.log(`${broker.address} successfully registered as a Broker`);
  } else {
    assert.strictEqual(
      role,
      'Broker',
      `Cannot register ${uri} as broker because it has a role: ${role}`,
    );
  }
}
