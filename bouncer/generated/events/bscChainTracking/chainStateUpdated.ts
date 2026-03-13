import { z } from 'zod';
import { cfChainsChainStateBsc } from '../common';

export const bscChainTrackingChainStateUpdated = z.object({ newChainState: cfChainsChainStateBsc });
