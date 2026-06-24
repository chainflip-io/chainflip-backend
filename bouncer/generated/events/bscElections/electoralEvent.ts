import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingBscElectionsBscElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingBscElectionsBscElectoralEvents;

export const bscElectionsElectoralEventEvent = defineEvent(
  'BscElections.ElectoralEvent',
  bscElectionsElectoralEvent,
);
