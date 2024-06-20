import { fineAmountToAmount, assetDecimals } from './utils';
import { getPolkadotApi } from './utils/substrate';

export async function getDotBalance(address: string): Promise<string> {
  await using polkadot = await getPolkadotApi();

  const planckBalance: string = (await polkadot.query.system.account(address)).data.free.toString();
  return fineAmountToAmount(planckBalance, assetDecimals('Dot'));
}
