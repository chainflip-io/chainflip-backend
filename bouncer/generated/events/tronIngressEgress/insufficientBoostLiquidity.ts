import { z } from 'zod';
import {
  cfChainsDepositOriginType,
  cfPrimitivesChainsAssetsTronAsset,
  numberOrHex,
} from '../common';

export const tronIngressEgressInsufficientBoostLiquidity = z.object({
  prewitnessedDepositId: numberOrHex,
  asset: cfPrimitivesChainsAssetsTronAsset,
  amountAttempted: numberOrHex,
  channelId: numberOrHex.nullish(),
  originType: cfChainsDepositOriginType,
});
