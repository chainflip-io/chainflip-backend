-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" ADD COLUMN     "expiryBlock" INTEGER,
ADD COLUMN     "isExpired" BOOLEAN NOT NULL DEFAULT false,
ADD COLUMN     "srcChainExpiryBlock" BIGINT;
