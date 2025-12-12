import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsDotAsset,
  numberOrHex,
} from '../common';

export const polkadotIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsDotAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
