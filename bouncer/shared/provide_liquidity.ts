import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { Asset } from '@chainflip/cli';
import {
  observeEvent,
  newAddress,
  getChainflipApi,
  decodeDotAddressForContract,
  handleSubstrateError,
  lpMutex,
  shortChainFromAsset,
  amountToFineAmount,
  isWithinOnePercent,
  chainFromAsset,
  chainContractId,
  assetDecimals,
} from '../shared/utils';
import { send } from '../shared/send';

export async function provideLiquidity(
  ccy: Asset,
  amount: number,
  waitForFinalization = false,
  lpKey?: string,
) {
  const chainflip = await getChainflipApi();
  await cryptoWaitReady();
  const chain = shortChainFromAsset(ccy);

  const keyring = new Keyring({ type: 'sr25519' });
  const lpUri = lpKey ?? (process.env.LP_URI || '//LP_1');
  const lp = keyring.createFromUri(lpUri);

  // If no liquidity refund address is registered, then do that now
  if (
    (
      await chainflip.query.liquidityProvider.liquidityRefundAddress(
        lp.address,
        chainContractId(chainFromAsset(ccy)),
      )
    ).toJSON() === null
  ) {
    let refundAddress = await newAddress(ccy, 'LP_1');
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
        BigInt(amountToFineAmount(String(amount), assetDecimals(ccy))),
      ),
    undefined,
    waitForFinalization,
  );
  await send(ccy, ingressAddress, String(amount));

  await eventHandle;
}
