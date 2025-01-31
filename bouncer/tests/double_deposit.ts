import { Keyring } from '../polkadot/keyring';
import { sleep, hexStringToBytesArray, newAddress, lpMutex } from '../shared/utils';
import { getChainflipApi, observeEvent } from '../shared/utils/substrate';
import { sendEvmNative } from '../shared/send_evm';
import { ExecutableTest } from '../shared/executable_test';

/* eslint-disable @typescript-eslint/no-use-before-define */
export const testDoubleDeposit = new ExecutableTest('DoubleDeposit', main, 220);

async function main(): Promise<void> {
  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI ?? '//LP_1';
  const lp = keyring.createFromUri(lpUri);
  await using chainflip = await getChainflipApi();

  // Register Liquidity Refund Address before requesting reposit address.
  const encodedEthAddr = chainflip.createType('EncodedAddress', {
    Eth: hexStringToBytesArray(await newAddress('Eth', 'LP_1')),
  });
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .registerLiquidityRefundAddress(encodedEthAddr)
      .signAndSend(lp);
  });

  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress('Eth', null)
      .signAndSend(lp);
  });
  const ethIngressKey = (
    await observeEvent('liquidityProvider:LiquidityDepositAddressReady', {
      test: (event) => event.data.depositAddress.Eth,
    }).event
  ).data.depositAddress.Eth as string;
  testDoubleDeposit.log('Eth ingress address: ' + ethIngressKey);
  await sleep(8000); // sleep for 8 seconds to give the engine a chance to start witnessing
  await sendEvmNative('Ethereum', ethIngressKey, '10');

  await observeEvent('assetBalances:AccountCredited').event;
  await sendEvmNative('Ethereum', ethIngressKey, '10');
  await observeEvent('assetBalances:AccountCredited').event;
}
