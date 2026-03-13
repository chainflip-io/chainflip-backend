import { z } from 'zod';
import { cfPrimitivesChainsAssetsBscAsset, numberOrHex } from '../common';

export const bscIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBscAsset,
});
