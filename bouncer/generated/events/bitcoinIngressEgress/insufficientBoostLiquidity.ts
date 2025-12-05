import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsBtcAsset,
  numberOrHex,
} from '../common';

export const bitcoinIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsBtcAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
