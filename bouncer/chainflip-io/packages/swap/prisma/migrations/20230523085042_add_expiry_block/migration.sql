/*
  Warnings:

  - You are about to drop the column `active` on the `SwapDepositChannel` table. All the data in the column will be lost.
  - Added the required column `expiryBlockHeight` to the `SwapDepositChannel` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" DROP COLUMN "active",
ADD COLUMN     "expiryBlock" INTEGER NOT NULL;
