use chainflip_common::types::chain::*;
use std::{fmt::Display, str::FromStr};

use crate::local_store::LocalEvent;

#[derive(Debug, Eq, PartialEq)]
pub enum TransactionType {
    PoolChange,
    SwapQuote,
    DepositQuote,
    Witness,
    Deposit,
    Output,
    Sent,
    WithdrawRequest,
    Withdraw,
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl FromStr for TransactionType {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PoolChange" => Ok(TransactionType::PoolChange),
            "SwapQuote" => Ok(TransactionType::SwapQuote),
            "DepositQuote" => Ok(TransactionType::DepositQuote),
            "Witness" => Ok(TransactionType::Witness),
            "Deposit" => Ok(TransactionType::Deposit),
            "Output" => Ok(TransactionType::Output),
            "Sent" => Ok(TransactionType::Sent),
            "Withdraw" => Ok(TransactionType::Withdraw),
            "WithdrawRequest" => Ok(TransactionType::WithdrawRequest),
            _ => Err("Invalid transaction type"),
        }
    }
}

impl From<&PoolChange> for TransactionType {
    fn from(_: &PoolChange) -> Self {
        TransactionType::PoolChange
    }
}

impl From<&SwapQuote> for TransactionType {
    fn from(_: &SwapQuote) -> Self {
        TransactionType::SwapQuote
    }
}

impl From<&DepositQuote> for TransactionType {
    fn from(_: &DepositQuote) -> Self {
        TransactionType::DepositQuote
    }
}

impl From<&Deposit> for TransactionType {
    fn from(_: &Deposit) -> Self {
        TransactionType::Deposit
    }
}

impl From<&Witness> for TransactionType {
    fn from(_: &Witness) -> Self {
        TransactionType::Witness
    }
}

impl From<&Output> for TransactionType {
    fn from(_: &Output) -> Self {
        TransactionType::Output
    }
}

impl From<&OutputSent> for TransactionType {
    fn from(_: &OutputSent) -> Self {
        TransactionType::Sent
    }
}

impl From<&WithdrawRequest> for TransactionType {
    fn from(_: &WithdrawRequest) -> Self {
        TransactionType::WithdrawRequest
    }
}

impl From<&Withdraw> for TransactionType {
    fn from(_: &Withdraw) -> Self {
        TransactionType::Withdraw
    }
}
