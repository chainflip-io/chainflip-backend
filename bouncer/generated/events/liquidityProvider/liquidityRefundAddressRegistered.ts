import { z } from 'zod';
import {
  accountId,
  cfChainsAddressForeignChainAddress,
  cfPrimitivesChainsForeignChain,
} from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const liquidityProviderLiquidityRefundAddressRegistered = z.object({
  accountId,
  chain: cfPrimitivesChainsForeignChain,
  address: cfChainsAddressForeignChainAddress,
});

export const liquidityProviderLiquidityRefundAddressRegisteredEvent = defineEvent(
  'LiquidityProvider.LiquidityRefundAddressRegistered',
  liquidityProviderLiquidityRefundAddressRegistered,
);
