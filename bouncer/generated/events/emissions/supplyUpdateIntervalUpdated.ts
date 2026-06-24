import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const emissionsSupplyUpdateIntervalUpdated = z.number();

export const emissionsSupplyUpdateIntervalUpdatedEvent = defineEvent(
  'Emissions.SupplyUpdateIntervalUpdated',
  emissionsSupplyUpdateIntervalUpdated,
);
