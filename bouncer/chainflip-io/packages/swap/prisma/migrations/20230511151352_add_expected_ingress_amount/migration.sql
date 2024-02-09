/*
  Warnings:

  - Added the required column `expectedIngressAmount` to the `SwapIntent` table without a default value. This is not possible if the table is not empty.

*/
-- AlterTable
ALTER TABLE "public"."SwapIntent" ADD COLUMN     "expectedIngressAmount" DECIMAL(30,0) NOT NULL;
