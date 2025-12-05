import { z } from 'zod';
import { palletCfSwappingPalletConfigUpdate } from '../common';

export const swappingPalletConfigUpdated = z.object({ update: palletCfSwappingPalletConfigUpdate });
