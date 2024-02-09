-- AlterTable
ALTER TABLE "public"."ChainTracking" ADD COLUMN     "blockTrackedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP;

-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" ADD COLUMN     "estimatedExpiryAt" TIMESTAMP(3);

-- This is the backfilling script to set some value on previous deposit channels,
-- otherwise previous ones will always be displayed as open
UPDATE "SwapDepositChannel" SET "estimatedExpiryAt" = "createdAt" + INTERVAL '2 hour' WHERE "expiryBlock" IS NOT NULL;