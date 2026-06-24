import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const validatorNewEpoch = z.number();

export const validatorNewEpochEvent = defineEvent('Validator.NewEpoch', validatorNewEpoch);
