import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { runWithTimeout, observeEvent, getChainflipApi, encodeBtcAddressForContract } from '../shared/utils';
import { fundBtc } from '../shared/fund_btc';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';


async function main(): Promise<void> {
  await cryptoWaitReady();
  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI ?? '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  const chainflip = await getChainflipApi();

  console.log('=== Testing expiry of funded LP deposit address ===');
  console.log('Setting expiry time for LP addresses to 10 blocks');

  await submitGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(10));
  await observeEvent('liquidityProvider:LpTtlSet', chainflip);

  console.log('Requesting new BTC LP deposit address');
  await chainflip.tx.liquidityProvider
    .requestLiquidityDepositAddress('Btc')
    .signAndSend(lp, { nonce: -1 });

  const depositEventResult = await observeEvent('liquidityProvider:LiquidityDepositAddressReady', chainflip);
  const ingressKey = depositEventResult[1].toJSON().btc;

  const ingressAddress = encodeBtcAddressForContract(ingressKey);

  console.log('Funding BTC LP deposit address of ' + ingressAddress + ' with 1 BTC');
  await fundBtc(ingressAddress, 1);
  await observeEvent('liquidityProvider:LiquidityDepositAddressExpired', chainflip);

  console.log('Setting expiry time for LP addresses to 100 blocks');
  await submitGovernanceExtrinsic(chainflip.tx.liquidityProvider.setLpTtl(100))
  await observeEvent('liquidityProvider:LpTtlSet', chainflip);
  console.log('=== Test complete ===');
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
