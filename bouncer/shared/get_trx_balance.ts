import { getEncodedTronAddress, getTronWebClient } from 'shared/utils';
import { TronWeb } from 'tronweb';

export async function getTrxBalance(address: string): Promise<string> {
  const tronWeb = getTronWebClient();
  const encodedAddress = getEncodedTronAddress(address);
  // Use getUnconfirmedBalance: our local node has solidityEnable=false, so the
  // confirmed endpoint (walletsolidity/getaccount) is unavailable.
  const sun = await tronWeb.trx.getUnconfirmedBalance(encodedAddress);
  return TronWeb.fromSun(sun) as string;
}
