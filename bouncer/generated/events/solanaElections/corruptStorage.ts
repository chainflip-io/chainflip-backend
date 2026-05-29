import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaElectionsCorruptStorage = z.null();

export const solanaElectionsCorruptStorageEvent = defineEvent(
  'SolanaElections.CorruptStorage',
  solanaElectionsCorruptStorage,
);
