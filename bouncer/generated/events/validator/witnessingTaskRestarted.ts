import { z } from 'zod';
import { accountId, cfPrimitivesWitnessingTaskName } from '../common';

export const validatorWitnessingTaskRestarted = z.object({
  task: cfPrimitivesWitnessingTaskName,
  reporter: accountId,
});
