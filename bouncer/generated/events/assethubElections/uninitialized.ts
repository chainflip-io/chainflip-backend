import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const assethubElectionsUninitialized = z.null();

export const assethubElectionsUninitializedEvent = defineEvent(
  'AssethubElections.Uninitialized',
  assethubElectionsUninitialized,
);
