import { InternalAsset as Asset } from '@chainflip/cli';
import {
  newAddress,
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
  createStateChainKeypair,
} from '../shared/utils';
import { send } from '../shared/send';
import { getChainflipApi, observeEvent } from './utils/substrate';
import { Logger } from './utils/logger';

export async function depositLiquidity(
  logger: Logger,
  ccy: Asset,
  amount: number,
  waitForFinalization = false,
  lpKey?: string,
) {
  await using chainflip = await getChainflipApi();
  const chain = shortChainFromAsset(ccy);

  const lp = createStateChainKeypair(lpKey ?? (process.env.LP_URI || '//LP_1'));

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

    logger.debug('Registering Liquidity Refund Address for ' + ccy + ': ' + refundAddress);
    await lpMutex.runExclusive(async () => {
      await chainflip.tx.liquidityProvider
        .registerLiquidityRefundAddress({ [chain]: refundAddress })
        .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
    });
  }

  let eventHandle = observeEvent(logger, 'liquidityProvider:LiquidityDepositAddressReady', {
    test: (event) => event.data.asset === ccy && event.data.accountId === lp.address,
  }).event;

  logger.debug('Requesting ' + ccy + ' deposit address');
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy, null)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  const ingressAddress = (await eventHandle).data.depositAddress[chain];

  logger.debug('Received ' + ccy + ' address: ' + ingressAddress);
  logger.debug('Sending ' + amount + ' ' + ccy + ' to ' + ingressAddress);
  eventHandle = observeEvent(logger, 'assetBalances:AccountCredited', {
    test: (event) =>
      event.data.asset === ccy &&
      event.data.accountId === lp.address &&
      isWithinOnePercent(
        BigInt(event.data.amountCredited.replace(/,/g, '')),
        BigInt(amountToFineAmount(String(amount), assetDecimals(ccy))),
      ),
    finalized: waitForFinalization,
  }).event;

  await send(logger, ccy, ingressAddress, String(amount));

  return eventHandle;
}
