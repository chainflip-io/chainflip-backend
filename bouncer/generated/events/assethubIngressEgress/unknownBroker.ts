import { z } from 'zod';
import { accountId } from '../common';

export const assethubIngressEgressUnknownBroker = z.object({ brokerId: accountId });
