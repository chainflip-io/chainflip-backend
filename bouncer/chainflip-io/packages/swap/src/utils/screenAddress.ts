import axios from 'axios';
import { z } from 'zod';
import logger from './logger';
import env from '../config/env';

const schema = z.object({ identifications: z.array(z.object({})) });

export default async function screenAddress(address: string): Promise<boolean> {
  const apiKey = env.CHAINALYSIS_API_KEY;

  if (!apiKey) return false;

  const response = await axios
    .get(`https://public.chainalysis.com/api/v1/address/${address}`, {
      headers: { 'X-API-Key': apiKey },
    })
    .catch(() => {
      logger.error('Failed to screen address');
      return { data: { identifications: [] } };
    });

  const result = schema.safeParse(response.data);

  if (!result.success) {
    logger.error('failed to parse chainalysis response', result.error);
    return false;
  }

  return result.data.identifications.length > 0;
}
