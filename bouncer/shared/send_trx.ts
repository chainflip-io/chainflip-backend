import { getEncodedTronAddress, getTronWebClient, getTronWhaleKeyPair } from 'shared/utils';
import { Logger } from 'shared/utils/logger';
import { TronWeb } from 'tronweb';
import { HexString } from '@polkadot/util/types';

export async function sendTrx(
  logger: Logger,
  toAddress: string,
  amount: string,
): Promise<HexString> {
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

  const txHash: HexString = `0x${result.transaction.txID}`;
  logger.info(`Transaction complete, tx_hash: ${txHash}`);

  return txHash;
}
