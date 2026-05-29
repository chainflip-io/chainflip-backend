import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinElectionsCorruptStorage = z.null();

export const bitcoinElectionsCorruptStorageEvent = defineEvent(
  'BitcoinElections.CorruptStorage',
  bitcoinElectionsCorruptStorage,
);
