/*
  Warnings:

  - You are about to drop the column `expiryBlock` on the `SwapDepositChannel` table. All the data in the column will be lost.

*/
-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" DROP COLUMN "expiryBlock";
