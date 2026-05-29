import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const witnesserPrewitnessed = z.object({ call: z.unknown() });

export const witnesserPrewitnessedEvent = defineEvent(
  'Witnesser.Prewitnessed',
  witnesserPrewitnessed,
);
