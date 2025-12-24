import { z } from 'zod';
import { cfPrimitivesChainsAssetsSolAsset, numberOrHex } from '../common';

export const solanaIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsSolAsset,
});
