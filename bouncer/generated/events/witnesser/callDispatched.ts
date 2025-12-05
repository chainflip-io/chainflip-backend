import { z } from 'zod';
import { hexString } from '../common';

export const witnesserCallDispatched = z.object({ callHash: hexString });
