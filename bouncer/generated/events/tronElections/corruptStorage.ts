import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const tronElectionsCorruptStorage = z.null();

export const tronElectionsCorruptStorageEvent = defineEvent(
  'TronElections.CorruptStorage',
  tronElectionsCorruptStorage,
);
