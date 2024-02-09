/*
  Warnings:

  - You are about to drop the `SwapDepositChannelBlock` table. If the table is not empty, all the data it contains will be lost.
  - Made the column `issuedBlock` on table `SwapDepositChannel` required. This step will fail if there are existing NULL values in that column.

*/
-- AlterTable
ALTER TABLE "public"."SwapDepositChannel" ALTER COLUMN "issuedBlock" SET NOT NULL;

-- DropTable
DROP TABLE "public"."SwapDepositChannelBlock";
