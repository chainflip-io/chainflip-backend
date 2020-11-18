use reqwest::StatusCode;

use crate::common::api::ResponseError;

macro_rules! bad_request {
    ($($arg:tt)*) => {
        ResponseError::new(reqwest::StatusCode::BAD_REQUEST, &format!($($arg)*))
    };
}

macro_rules! internal_error {
    ($($arg:tt)*) => {
        ResponseError::new(reqwest::StatusCode::INTERNAL_SERVER_ERROR, &format!($($arg)*))
    };
}

/// Bad request response error
pub fn bad_request(message: &str) -> ResponseError {
    ResponseError::new(StatusCode::BAD_REQUEST, message)
}

/// Internal server response error
pub fn internal_server_error() -> ResponseError {
    ResponseError::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}
