import { z } from 'zod';
import {
  accountId,
  cfChainsAddressEncodedAddress,
  cfPrimitivesChainsAssetsAnyAsset,
  cfPrimitivesChainsForeignChain,
  numberOrHex,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderAssetBalancePurged = z.object({
  accountId,
  asset: cfPrimitivesChainsAssetsAnyAsset,
  amount: numberOrHex,
  egressId: z.tuple([cfPrimitivesChainsForeignChain, numberOrHex]),
  destinationAddress: cfChainsAddressEncodedAddress,
  fee: numberOrHex,
});

export const liquidityProviderAssetBalancePurgedEvent = defineEvent(
  'LiquidityProvider.AssetBalancePurged',
  liquidityProviderAssetBalancePurged,
);
