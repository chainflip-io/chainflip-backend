import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset, chainContractIds, assetDecimals } from '@chainflip-io/cli';
import {
  observeEvent,
  newAddress,
  getChainflipApi,
  decodeDotAddressForContract,
  handleSubstrateError,
  lpMutex,
  assetToChain,
  amountToFineAmount,
  isWithinOnePercent,
  chainFromAsset,
} from '../shared/utils';
import { send } from '../shared/send';

export async function provideLiquidity(ccy: Asset, amount: number, waitForFinalization = false) {
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();
  const chain = assetToChain(ccy);

  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = process.env.LP_URI || '//LP_1';
  const lp = keyring.createFromUri(lpUri);

  // If no liquidity refund address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.liquidityRefundAddress(
        lp.address,
        chainContractIds[chainFromAsset(ccy)],
      )
    ).toJSON() === null
  ) {
    let refundAddress = await newAddress(chain.toUpperCase() as Asset, 'LP_1');
    refundAddress = ccy === 'DOT' ? decodeDotAddressForContract(refundAddress) : refundAddress;

    console.log('Registering Liquidity Refund Address for ' + ccy + ': ' + refundAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerLiquidityRefundAddress({ [chain]: refundAddress })
        .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
    });
  }

  let eventHandle = observeEvent(
    'liquidityProvider:LiquidityDepositAddressReady',
    chainflip,
    (event) => event.data.asset.toUpperCase() === ccy,
  );

  console.log('Requesting ' + ccy + ' deposit address');
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy.toLowerCase(), null)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  const ingressAddress = (await eventHandle).data.depositAddress[chain];

  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  console.log('Sending ' + amount + ' ' + ccy + ' to ' + ingressAddress);
  eventHandle = observeEvent(
    'liquidityProvider:AccountCredited',
    chainflip,
    (event) =>
      event.data.asset.toUpperCase() === ccy &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(amountToFineAmount(String(amount), assetDecimals[ccy])),
      ),
    undefined,
    waitForFinalization,
  );
  await send(ccy, ingressAddress, String(amount));

  await eventHandle;
}
