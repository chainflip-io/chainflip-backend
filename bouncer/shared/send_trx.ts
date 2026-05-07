import { getEncodedTronAddress, getTronWebClient, getTronWhaleKeyPair } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { TronWeb } from 'tronweb';

export async function sendTrx(logger: Logger, toAddress: string, amount: string): Promise<void> {
  const tronWeb = getTronWebClient();
  const sun = Number(TronWeb.toSun(parseFloat(amount)));

  logger.info(`Transferring ${amount} TRX to ${toAddress}`);

  const encodedAddress = getEncodedTronAddress(toAddress);

  const result = await tronWeb.trx.sendTransaction(encodedAddress, sun, {
    privateKey: getTronWhaleKeyPair().privkey,
  });

  if (!result.result) {
    throw new Error(`Transaction failed: ${JSON.stringify(result)}`);
  }

  logger.info(`Transaction complete, tx_hash: ${result.transaction.txID}`);
}
