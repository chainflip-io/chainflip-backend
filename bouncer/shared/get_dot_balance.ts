import { assetDecimals, Asset } from '@chainflip-io/cli';
import { getPolkadotApi, fineAmountToAmount } from './utils';

export async function getDotBalance(address: string): Promise<string> {
  const polkadot = await getPolkadotApi(process.env.POLKADOT_ENDPOINT);

  const planckBalance: string = (await polkadot.query.system.account(address)).data.free.toString();
  return fineAmountToAmount(planckBalance, assetDecimals['DOT' as Asset]);
}
