/*
  Warnings:

  - Added the required column `srcAmount` to the `Swap` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "destAmount" DECIMAL(30,0),
ADD COLUMN     "srcAmount" DECIMAL(30,0) ;

-- backfill data
UPDATE "public"."Swap" SET "srcAmount" = "depositAmount";

-- add not null constraint
ALTER TABLE "public"."Swap" ALTER COLUMN "srcAmount" SET NOT NULL;
