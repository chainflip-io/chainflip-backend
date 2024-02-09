/*
  Warnings:

  - You are about to drop the column `egressAmount` on the `Swap` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "public"."Swap" DROP COLUMN "egressAmount";
