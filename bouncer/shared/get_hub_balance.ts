import { fineAmountToAmount, assetDecimals, HubAsset, getHubAssetId } from 'shared/utils';
import { getAssethubApi } from 'shared/utils/substrate';
import { globalLogger } from './utils/logger';

export async function getHubDotBalance(address: string): Promise<string> {
  await using assethub = await getAssethubApi();

  const fineAmountBalance: string = // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ((await assethub.query.system.account(address)) as any).data.free.toString();
  return fineAmountToAmount(fineAmountBalance, assetDecimals('HubDot'));
}

export async function getHubAssetBalance(asset: HubAsset, address: string): Promise<string> {
  await using assethub = await getAssethubApi();

  const reply = await assethub.query.assets.account(getHubAssetId(asset), address);

  if (reply.isEmpty) {
    globalLogger.warn(
      `Empty reply from assetHub account query for asset ${asset}, address ${address}`,
    );
    return '0';
  }

  const fineAmountBalance = // eslint-disable-next-line @typescript-eslint/no-explicit-any
    JSON.parse(reply as any).balance;
  return fineAmountToAmount(fineAmountBalance, assetDecimals(asset));
}
