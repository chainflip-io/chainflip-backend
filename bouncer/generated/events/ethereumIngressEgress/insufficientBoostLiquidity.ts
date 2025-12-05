import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsEthAsset,
  numberOrHex,
} from '../common';

export const ethereumIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsEthAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
