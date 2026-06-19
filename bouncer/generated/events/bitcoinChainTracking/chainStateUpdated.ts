import { z } from 'zod';
import { cfChainsChainStateBitcoin } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinChainTrackingChainStateUpdated = z.object({
  newChainState: cfChainsChainStateBitcoin,
});

export const bitcoinChainTrackingChainStateUpdatedEvent = defineEvent(
  'BitcoinChainTracking.ChainStateUpdated',
  bitcoinChainTrackingChainStateUpdated,
);
