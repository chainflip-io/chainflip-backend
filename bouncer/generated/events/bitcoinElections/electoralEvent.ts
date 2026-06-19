import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingBitcoinElectionsBitcoinElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingBitcoinElectionsBitcoinElectoralEvents;

export const bitcoinElectionsElectoralEventEvent = defineEvent(
  'BitcoinElections.ElectoralEvent',
  bitcoinElectionsElectoralEvent,
);
