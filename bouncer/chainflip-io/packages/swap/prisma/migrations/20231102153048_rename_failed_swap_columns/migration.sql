/*
  Warnings:

  - You are about to drop the column `destinationAddress` on the `FailedSwap` table. All the data in the column will be lost.
  - You are about to drop the column `destinationChain` on the `FailedSwap` table. All the data in the column will be lost.
  - You are about to drop the column `sourceChain` on the `FailedSwap` table. All the data in the column will be lost.
  - Added the required column `destAddress` to the `FailedSwap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `destChain` to the `FailedSwap` table without a default value. This is not possible if the table is not empty.
  - Added the required column `srcChain` to the `FailedSwap` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."FailedSwap" RENAME COLUMN "destinationAddress" TO "destAddress";
ALTER TABLE "public"."FailedSwap" RENAME COLUMN "destinationChain" TO "destChain";
ALTER TABLE "public"."FailedSwap" RENAME COLUMN "sourceChain" TO "srcChain";
