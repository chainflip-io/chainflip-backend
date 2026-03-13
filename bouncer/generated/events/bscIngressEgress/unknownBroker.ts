import { z } from 'zod';
import { accountId } from '../common';

export const bscIngressEgressUnknownBroker = z.object({ brokerId: accountId });
