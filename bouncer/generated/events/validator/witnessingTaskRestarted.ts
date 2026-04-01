import { z } from 'zod';
import { accountId, cfPrimitivesWitnessingTask } from '../common';

export const validatorWitnessingTaskRestarted = z.object({
  task: cfPrimitivesWitnessingTask,
  reporter: accountId,
});
