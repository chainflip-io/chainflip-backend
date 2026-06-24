import { z } from 'zod';
import { stateChainRuntimeChainflipWitnessingTronElectionsTronElectoralEvents } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronElectionsElectoralEvent =
  stateChainRuntimeChainflipWitnessingTronElectionsTronElectoralEvents;

export const tronElectionsElectoralEventEvent = defineEvent(
  'TronElections.ElectoralEvent',
  tronElectionsElectoralEvent,
);
