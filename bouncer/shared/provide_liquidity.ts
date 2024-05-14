import { InternalAsset as Asset } from '@chainflip/cli';
import { Keyring } from '../polkadot/keyring';
import {
  newAddress,
  getChainflipApi,
  decodeDotAddressForContract,
  handleSubstrateError,
  lpMutex,
  shortChainFromAsset,
  amountToFineAmount,
  isWithinOnePercent,
  chainFromAsset,
  decodeSolAddress,
  chainContractId,
  assetDecimals,
} from '../shared/utils';
import { send } from '../shared/send';
import { observeEvent } from './utils/substrate';

export async function provideLiquidity(
  ccy: Asset,
  amount: number,
  waitForFinalization = false,
  lpKey?: string,
) {
  await using chainflip = await getChainflipApi();
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
    refundAddress = chain === 'Dot' ? decodeDotAddressForContract(refundAddress) : refundAddress;
    refundAddress = chain === 'Sol' ? decodeSolAddress(refundAddress) : refundAddress;

    console.log('Registering Liquidity Refund Address for ' + ccy + ': ' + refundAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerLiquidityRefundAddress({ [chain]: refundAddress })
        .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
    });
  }

  let eventHandle = observeEvent('liquidityProvider:LiquidityDepositAddressReady', {
    test: (event) => event.data.asset === ccy,
  });

  console.log('Requesting ' + ccy + ' deposit address');
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy, null)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  const ingressAddress = (await eventHandle).data.depositAddress[chain];

  console.log('Received ' + ccy + ' address: ' + ingressAddress);
  console.log('Sending ' + amount + ' ' + ccy + ' to ' + ingressAddress);
  eventHandle = observeEvent('liquidityProvider:AccountCredited', {
    test: (event) =>
      event.data.asset === ccy &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(amountToFineAmount(String(amount), assetDecimals(ccy))),
      ),
    finalized: waitForFinalization,
  });
  await send(ccy, ingressAddress, String(amount));

  await eventHandle;
}
