import { z } from 'zod';
import { accountId } from '../common';

export const arbitrumIngressEgressUnknownBroker = z.object({ brokerId: accountId });
