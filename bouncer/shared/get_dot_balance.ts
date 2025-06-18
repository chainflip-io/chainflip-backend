import { fineAmountToAmount, assetDecimals } from 'shared/utils';
import { getPolkadotApi } from 'shared/utils/substrate';

export async function getDotBalance(address: string): Promise<string> {
  await using polkadot = await getPolkadotApi();

  const planckBalance: string = // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ((await polkadot.query.system.account(address)) as any).data.free.toString();
  return fineAmountToAmount(planckBalance, assetDecimals('Dot'));
}
