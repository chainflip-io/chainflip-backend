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
  assetDecimals,
  createStateChainKeypair,
} from '../shared/utils';
import { send } from '../shared/send';
import { Event, getChainflipApi, observeEvent } from './utils/substrate';
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
        chainFromAsset(ccy),
      )
    ).toJSON() === null
  ) {
    let refundAddress = await newAddress(ccy, 'LP_1');
    refundAddress =
      chain === 'Dot' || chain === 'Hub'
        ? decodeDotAddressForContract(refundAddress)
        : refundAddress;
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

  logger.debug(`Requesting ${ccy} deposit address`);
  await lpMutex.runExclusive(async () => {
    await chainflip.tx.liquidityProvider
      .requestLiquidityDepositAddress(ccy, null)
      .signAndSend(lp, { nonce: -1 }, handleSubstrateError(chainflip));
  });

  const ingressAddress = (await eventHandle).data.depositAddress[chain];

  logger.trace(`Received ${ccy} deposit address: ${ingressAddress}`);
  logger.trace(`Initiating transfer of ${amount} ${ccy} to ${ingressAddress}`);

  function checkAccountCreditedEvent(event: Event): boolean {
    if (event.data.asset === ccy && event.data.accountId === lp.address) {
      const expectedAmount = BigInt(amountToFineAmount(String(amount), assetDecimals(ccy)));
      if (isWithinOnePercent(BigInt(event.data.amountCredited.replace(/,/g, '')), expectedAmount)) {
        return true;
      }
      logger.warn(
        `Account credited event amount mismatch: expected within 1% of ${expectedAmount}, got ${event.data.amountCredited} ${ccy}`,
      );
    }
    return false;
  }
  eventHandle = observeEvent(logger, 'assetBalances:AccountCredited', {
    test: checkAccountCreditedEvent,
    finalized: waitForFinalization,
  }).event;

  await send(logger, ccy, ingressAddress, String(amount));

  await eventHandle;
  logger.debug(`Liquidity deposited: ${amount} ${ccy} to ${ingressAddress}`);
}
