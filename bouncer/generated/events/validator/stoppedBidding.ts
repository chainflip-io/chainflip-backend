import { z } from 'zod';
import { accountId } from '../common';

export const validatorStoppedBidding = z.object({ accountId });
