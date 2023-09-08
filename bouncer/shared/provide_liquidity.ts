import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset, assetChains, chainContractIds } from '@chainflip-io/cli';
import {
  observeEvent,
  newAddress,
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

  // If no liquidity refund address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.liquidityRefundAddress(
        lp.address,
        chainContractIds[assetChains[ccy]],
      )
    ).toJSON() === null
  ) {
    let refundAddress = await newAddress(assetToChain(ccy).toUpperCase() as Asset, 'LP_1');
    refundAddress = ccy === 'DOT' ? decodeDotAddressForContract(refundAddress) : refundAddress;

    console.log('Registering Liquidity Refund Address for ' + ccy + ': ' + refundAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerliquidityRefundAddress({ [assetToChain(ccy)]: refundAddress })
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
