import { z } from 'zod';
import { spRuntimeDispatchError } from '../common';

export const swappingFlipTransferToGatewaySkipped = z.object({ reason: spRuntimeDispatchError });
