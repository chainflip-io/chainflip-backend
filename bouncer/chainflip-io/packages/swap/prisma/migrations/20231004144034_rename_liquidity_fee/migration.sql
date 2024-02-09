/*
  Warnings:

  - You are about to drop the column `feeHundredthPips` on the `Pool` table. All the data in the column will be lost.
  - Added the required column `liquidityFeeHundredthPips` to the `Pool` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."Pool" RENAME COLUMN "feeHundredthPips" TO "liquidityFeeHundredthPips";
