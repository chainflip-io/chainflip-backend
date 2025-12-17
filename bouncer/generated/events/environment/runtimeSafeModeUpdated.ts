import { z } from 'zod';
import { palletCfEnvironmentSafeModeUpdate } from '../common';

export const environmentRuntimeSafeModeUpdated = z.object({
  safeMode: palletCfEnvironmentSafeModeUpdate,
});
