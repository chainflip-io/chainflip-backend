import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const emissionsCurrentAuthorityInflationEmissionsUpdated = z.number();

export const emissionsCurrentAuthorityInflationEmissionsUpdatedEvent = defineEvent(
  'Emissions.CurrentAuthorityInflationEmissionsUpdated',
  emissionsCurrentAuthorityInflationEmissionsUpdated,
);
