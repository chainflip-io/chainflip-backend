import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const bscElectionsUninitialized = z.null();

export const bscElectionsUninitializedEvent = defineEvent(
  'BscElections.Uninitialized',
  bscElectionsUninitialized,
);
