import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinElectionsUninitialized = z.null();

export const bitcoinElectionsUninitializedEvent = defineEvent(
  'BitcoinElections.Uninitialized',
  bitcoinElectionsUninitialized,
);
