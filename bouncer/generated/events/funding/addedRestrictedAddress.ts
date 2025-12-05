import { z } from 'zod';
import { hexString } from '../common';

export const fundingAddedRestrictedAddress = z.object({ address: hexString });
