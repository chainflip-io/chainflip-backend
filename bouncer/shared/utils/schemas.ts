import z from 'zod';

export const numericString = z
  .string()
  .regex(/^[\d,]+$/)
  .transform((n) => Number(n.replaceAll(',', '')));

export const hexString = z
  .string()
  .refine((string): string is `0x${string}` => /^0x[a-fA-F0-9]*$/.test(string));
