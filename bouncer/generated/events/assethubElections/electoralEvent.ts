import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingAssethubElectionsAssethubElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingAssethubElectionsAssethubElectoralEvents;

export const assethubElectionsElectoralEventEvent = defineEvent(
  'AssethubElections.ElectoralEvent',
  assethubElectionsElectoralEvent,
);
