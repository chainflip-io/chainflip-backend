import { z } from 'zod';
import { accountId } from '../common';

export const tronIngressEgressUnknownBroker = z.object({ brokerId: accountId });
