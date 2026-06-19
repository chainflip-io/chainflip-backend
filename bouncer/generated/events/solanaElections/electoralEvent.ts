import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaElectionsElectoralEvent = z.null();

export const solanaElectionsElectoralEventEvent = defineEvent(
  'SolanaElections.ElectoralEvent',
  solanaElectionsElectoralEvent,
);
