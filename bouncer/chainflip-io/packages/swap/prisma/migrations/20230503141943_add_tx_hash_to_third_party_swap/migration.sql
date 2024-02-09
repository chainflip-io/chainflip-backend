/*
  Warnings:

  - Added the required column `txHash` to the `ThirdPartySwap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `txLink` to the `ThirdPartySwap` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."ThirdPartySwap" ADD COLUMN     "txHash" TEXT NOT NULL,
ADD COLUMN     "txLink" TEXT NOT NULL;
