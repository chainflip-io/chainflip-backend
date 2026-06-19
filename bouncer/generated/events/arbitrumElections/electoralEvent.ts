import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingArbitrumElectionsArbitrumElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingArbitrumElectionsArbitrumElectoralEvents;

export const arbitrumElectionsElectoralEventEvent = defineEvent(
  'ArbitrumElections.ElectoralEvent',
  arbitrumElectionsElectoralEvent,
);
