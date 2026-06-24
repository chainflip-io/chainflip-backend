import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumElectionsUninitialized = z.null();

export const arbitrumElectionsUninitializedEvent = defineEvent(
  'ArbitrumElections.Uninitialized',
  arbitrumElectionsUninitialized,
);
