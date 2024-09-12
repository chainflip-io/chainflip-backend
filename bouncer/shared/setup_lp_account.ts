import { lpMutex, handleSubstrateError, createLpKeypair } from './utils';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { fundFlip } from './fund_flip';

export async function setupLpAccount(lpKey: string) {
  const lp = createLpKeypair(lpKey);

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
