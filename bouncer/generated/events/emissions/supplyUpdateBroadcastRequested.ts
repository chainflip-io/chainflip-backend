import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const emissionsSupplyUpdateBroadcastRequested = z.number();

export const emissionsSupplyUpdateBroadcastRequestedEvent = defineEvent(
  'Emissions.SupplyUpdateBroadcastRequested',
  emissionsSupplyUpdateBroadcastRequested,
);
