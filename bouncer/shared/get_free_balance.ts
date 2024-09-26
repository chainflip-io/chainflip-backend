import { getChainflipApi } from './utils/substrate';
import { InternalAsset as Asset } from '@chainflip/cli';

export async function getFreeBalance(address: string, asset: Asset): Promise<bigint> {
  await using chainflip = await getChainflipApi();
  const fee = await chainflip.query.assetBalances.freeBalances(address, asset);
  // If the option is none we assume the balance is 0 for tests.
  if (fee.isEmpty) {
    return BigInt(0);
  } else {
    return BigInt(JSON.parse(fee.toString()).amount);
  }
}
