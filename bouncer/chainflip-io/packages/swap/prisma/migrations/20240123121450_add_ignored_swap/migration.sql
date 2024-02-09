/*
  Warnings:

  - Added the required column `type` to the `FailedSwap` table without a default value. This is not possible if the table is not empty.

*/
-- CreateEnum
BEGIN;
CREATE TYPE "public"."FailedSwapType" AS ENUM ('FAILED', 'IGNORED');

-- CreateEnum
CREATE TYPE "public"."FailedSwapReason" AS ENUM ('BelowMinimumDeposit', 'NotEnoughToPayFees', 'EgressAmountZero');

-- AlterTable
ALTER TABLE "public"."FailedSwap" ADD COLUMN     "reason" "public"."FailedSwapReason",
ADD COLUMN     "type" "public"."FailedSwapType" NOT NULL default 'FAILED';
ALTER TABLE "public"."FailedSwap" ALTER COLUMN "type" DROP DEFAULT;
COMMIT;
