import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const solanaElectionsUninitialized = z.null();

export const solanaElectionsUninitializedEvent = defineEvent(
  'SolanaElections.Uninitialized',
  solanaElectionsUninitialized,
);
