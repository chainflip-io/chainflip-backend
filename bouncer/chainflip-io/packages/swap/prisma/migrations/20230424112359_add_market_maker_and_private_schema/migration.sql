/*
  Warnings:

  - You are about to drop the `State` table. If the table is not empty, all the data it contains will be lost.

*/
-- CreateSchema
CREATE SCHEMA IF NOT EXISTS "private";

-- CreateTable
CREATE TABLE "private"."State" (
    "id" SERIAL NOT NULL,
    "height" INTEGER NOT NULL DEFAULT 0,

    CONSTRAINT "State_pkey" PRIMARY KEY ("id")
);

-- migrate data from public to private
INSERT INTO "private"."State" ("id", "height")
SELECT "id", "height" FROM "public"."State";

-- DropTable
DROP TABLE "public"."State";

-- CreateTable
CREATE TABLE "private"."MarketMaker" (
    "id" SERIAL NOT NULL,
    "name" TEXT NOT NULL,
    "sharedSecret" TEXT NOT NULL,

    CONSTRAINT "MarketMaker_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "MarketMaker_name_key" ON "private"."MarketMaker"("name");

-- CreateIndex
CREATE UNIQUE INDEX "MarketMaker_sharedSecret_key" ON "private"."MarketMaker"("sharedSecret");
