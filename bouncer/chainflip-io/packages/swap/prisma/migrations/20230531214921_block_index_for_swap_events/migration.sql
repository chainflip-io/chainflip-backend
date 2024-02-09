/*
  Warnings:

  - Added the required column `depositReceivedBlockIndex` to the `Swap` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."Swap" ADD COLUMN     "depositReceivedBlockIndex" TEXT NOT NULL,
ADD COLUMN     "egressCompletedBlockIndex" TEXT,
ADD COLUMN     "swapExecutedBlockIndex" TEXT;
