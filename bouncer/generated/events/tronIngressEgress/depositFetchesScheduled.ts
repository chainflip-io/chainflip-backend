import { z } from 'zod';
import { cfPrimitivesChainsAssetsTronAsset, numberOrHex } from '../common';

export const tronIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsTronAsset,
});
