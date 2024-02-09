-- CreateEnum
CREATE TYPE "ThirdPartyProtocol" AS ENUM ('Lifi', 'Squid');

-- CreateTable
CREATE TABLE "ThirdPartySwap" (
    "id" BIGSERIAL NOT NULL,
    "uuid" TEXT NOT NULL,
    "protocol" "ThirdPartyProtocol" NOT NULL,
    "routeResponse" JSONB NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "ThirdPartySwap_pkey" PRIMARY KEY ("id")
);

-- CreateIndex
CREATE UNIQUE INDEX "ThirdPartySwap_uuid_key" ON "ThirdPartySwap"("uuid");
