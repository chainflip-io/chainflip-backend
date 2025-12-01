import { z } from 'zod';
import { accountId } from '../common';

export const ethereumIngressEgressUnknownBroker = z.object({ brokerId: accountId });
