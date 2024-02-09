-- AlterTable
ALTER TABLE "public"."Swap" RENAME COLUMN "depositAsset" TO "srcAsset";
ALTER TABLE "public"."Swap" RENAME COLUMN "destinationAddress" TO "destAddress";
ALTER TABLE "public"."Swap" RENAME COLUMN "destinationAsset" TO "destAsset";

-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" RENAME COLUMN "depositAsset" TO "srcAsset";
ALTER TABLE "public"."SwapDepositChannel" RENAME COLUMN "destinationAddress" TO "destAddress";
ALTER TABLE "public"."SwapDepositChannel" RENAME COLUMN "destinationAsset" TO "destAsset";
