import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingGenericElectionsGenericElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const genericElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingGenericElectionsGenericElectoralEvents;

export const genericElectionsElectoralEventEvent = defineEvent(
  'GenericElections.ElectoralEvent',
  genericElectionsElectoralEvent,
);
