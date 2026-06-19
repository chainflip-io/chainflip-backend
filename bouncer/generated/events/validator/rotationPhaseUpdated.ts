import { z } from 'zod';
import { palletCfValidatorRotationPhase } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const validatorRotationPhaseUpdated = z.object({ newPhase: palletCfValidatorRotationPhase });

export const validatorRotationPhaseUpdatedEvent = defineEvent(
  'Validator.RotationPhaseUpdated',
  validatorRotationPhaseUpdated,
);
