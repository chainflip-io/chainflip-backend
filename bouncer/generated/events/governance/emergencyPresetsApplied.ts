import { z } from 'zod';
import { accountId, cfTraitsSafeModeEmergencyPreset } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const governanceEmergencyPresetsApplied = z.object({
  member: accountId,
  presets: z.array(cfTraitsSafeModeEmergencyPreset),
});

export const governanceEmergencyPresetsAppliedEvent = defineEvent(
  'Governance.EmergencyPresetsApplied',
  governanceEmergencyPresetsApplied,
);
