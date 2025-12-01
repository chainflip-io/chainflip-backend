import { z } from 'zod';
import { palletCfFlipPalletConfigUpdate } from '../common';

export const flipPalletConfigUpdated = z.object({ update: palletCfFlipPalletConfigUpdate });
