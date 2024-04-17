import { getPolkadotApi, fineAmountToAmount, assetDecimals } from './utils';

export async function getDotBalance(address: string): Promise<string> {
  await using polkadot = await getPolkadotApi(process.env.POLKADOT_ENDPOINT);

  const planckBalance: string = (await polkadot.query.system.account(address)).data.free.toString();
  return fineAmountToAmount(planckBalance, assetDecimals('Dot'));
}
