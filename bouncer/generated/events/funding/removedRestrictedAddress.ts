import { z } from 'zod';
import { hexString } from '../common';

export const fundingRemovedRestrictedAddress = z.object({ address: hexString });
