/*
  Warnings:

  - You are about to drop the column `destAmount` on the `Swap` table. All the data in the column will be lost.
  - You are about to drop the column `srcAmount` on the `Swap` table. All the data in the column will be lost.
  - Added the required column `swapInputAmount` to the `Swap` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."Swap" RENAME COLUMN "srcAmount" TO "swapInputAmount";
ALTER TABLE "public"."Swap" RENAME COLUMN "destAmount" TO "swapOutputAmount";