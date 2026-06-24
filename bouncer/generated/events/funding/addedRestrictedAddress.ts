import { z } from 'zod';
import { hexString } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const fundingAddedRestrictedAddress = z.object({ address: hexString });

export const fundingAddedRestrictedAddressEvent = defineEvent(
  'Funding.AddedRestrictedAddress',
  fundingAddedRestrictedAddress,
);
