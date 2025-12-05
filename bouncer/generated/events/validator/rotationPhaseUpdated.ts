import { z } from 'zod';
import { palletCfValidatorRotationPhase } from '../common';

export const validatorRotationPhaseUpdated = z.object({ newPhase: palletCfValidatorRotationPhase });
