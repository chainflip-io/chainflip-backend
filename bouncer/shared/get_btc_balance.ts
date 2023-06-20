import { getBtcClient } from "./utils";

const client = getBtcClient(process.env.BTC_ENDPOINT);

export async function getBtcBalance(bitcoinAddress: string): Promise<number> {
    const result = await client.listReceivedByAddress(1, false, true, bitcoinAddress);
    return result[0]?.amount || 0;
}