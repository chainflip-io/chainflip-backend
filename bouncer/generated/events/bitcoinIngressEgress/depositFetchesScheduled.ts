import { z } from 'zod';
import { cfPrimitivesChainsAssetsBtcAsset, numberOrHex } from '../common';

export const bitcoinIngressEgressDepositFetchesScheduled = z.object({
  channelId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBtcAsset,
});
