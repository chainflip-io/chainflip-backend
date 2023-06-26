import { getPolkadotApi } from "./utils";

export async function getDotBalance(address: string): Promise<string> {

    const polkadot = await getPolkadotApi(process.env.POLKADOT_ENDPOINT);

    const planckBalance: string = (await polkadot.query.system.account(address)).data.free.toString();
    const balanceLen = planckBalance.length;
    let balance;
    if (balanceLen > 10) {
        const decimalLocation = balanceLen - 10;
        balance = planckBalance.slice(0, decimalLocation) + '.' + planckBalance.slice(decimalLocation);
    } else {
        balance = '0.' + planckBalance.padStart(10, '0');
    }

    return balance
}