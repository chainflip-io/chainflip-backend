import { getBtcClient } from './utils';

export async function getBtcBalance(bitcoinAddress: string): Promise<number> {
  const client = getBtcClient();
  const result = await client.listReceivedByAddress(1, false, true, bitcoinAddress);
  return result[0]?.amount || 0;
}
