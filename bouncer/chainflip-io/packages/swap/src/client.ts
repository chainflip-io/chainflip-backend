// eslint-disable-next-line import/no-extraneous-dependencies
import { PrismaClient } from '.prisma/client';

// eslint-disable-next-line import/no-extraneous-dependencies
export * from '.prisma/client';

const prisma = new PrismaClient();

export default prisma;
