import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset, assetChains, chainContractIds } from '@chainflip-io/cli';
import {
  observeEvent,
  getAddress,
  getChainflipApi,
  decodeDotAddressForContract,
  handleSubstrateError,
  lpMutex,
  assetToChain,
} from '../shared/utils';
import { send } from '../shared/send';

export async function provideLiquidity(ccy: Asset, amount: number) {
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  // If no emergency withdrawal address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.emergencyWithdrawalAddress(
        lp.address,
        chainContractIds[assetChains[ccy]],
      )
    ).toJSON() === null
  ) {
    let emergencyAddress = await getAddress(ccy, 'LP_1');
    emergencyAddress =
      ccy === 'DOT' ? decodeDotAddressForContract(emergencyAddress) : emergencyAddress;

    console.log('Registering Emergency Withdrawal Address for ' + ccy + ': ' + emergencyAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerEmergencyWithdrawalAddress({ [assetToChain(ccy)]: emergencyAddress })
        .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
    });
  }

  console.log('Requesting ' + ccy + ' deposit address');
  let eventHandle =
    assetToChain(ccy) === 'Eth'
      ? observeEvent(
          'ethereumIngressEgress:StartWitnessing',
          chainflip,
          (event) => event.data.sourceAsset.toUpperCase() === ccy,
        )
      : observeEvent(
          'liquidityProvider:LiquidityDepositAddressReady',
          chainflip,
          (event) => event.data.depositAddress[assetToChain(ccy)] !== undefined,
        );
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy.toLowerCase())
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });
  const ingressAddress =
    assetToChain(ccy) === 'Eth'
      ? (await eventHandle).data.depositAddress
      : (await eventHandle).data.depositAddress[assetToChain(ccy)];

  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  console.log('Sending ' + amount + ' ' + ccy + ' to ' + ingressAddress);
  eventHandle = observeEvent(
    'liquidityProvider:AccountCredited',
    chainflip,
    (event) => event.data.asset.toUpperCase() === ccy,
  );
  send(ccy, ingressAddress, String(amount));
  await eventHandle;
}
