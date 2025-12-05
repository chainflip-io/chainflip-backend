import { z } from 'zod';
import { accountId } from '../common';

export const solanaIngressEgressUnknownBroker = z.object({ brokerId: accountId });
