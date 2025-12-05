import { z } from 'zod';
import { cfPrimitivesChainsAssetsHubAsset, numberOrHex } from '../common';

export const assethubIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsHubAsset,
});
