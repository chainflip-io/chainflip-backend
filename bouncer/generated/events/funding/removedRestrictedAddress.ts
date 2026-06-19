import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingRemovedRestrictedAddress = z.object({ address: hexString });

export const fundingRemovedRestrictedAddressEvent = defineEvent(
  'Funding.RemovedRestrictedAddress',
  fundingRemovedRestrictedAddress,
);
