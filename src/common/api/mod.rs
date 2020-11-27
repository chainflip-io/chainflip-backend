use serde;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::fmt;
use std::future::Future;
use warp::{reject::Reject, Filter};

/// A representation of a response error.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ResponseError {
    /// The error code
    pub code: u16,
    /// The error message
    pub message: String,
}

impl ResponseError {
    /// Create an API Error from a warp http status code
    pub fn new(code: warp::http::StatusCode, message: &str) -> Self {
        ResponseError {
            code: code.as_u16(),
            message: message.to_owned(),
        }
    }
}

impl std::error::Error for ResponseError {}

impl fmt::Display for ResponseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl Reject for ResponseError {}

/// A representation of the API response
#[derive(Debug, Serialize, Deserialize)]
pub struct Response<T> {
    /// Whether this response was a success
    pub success: bool,
    /// The data associated with this response if successful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// The error associated with this response if not successful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

impl<T> Response<T> {
    /// Create a success response
    pub fn success(data: T) -> Self {
        Response {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create a failure response
    pub fn failure(error: ResponseError) -> Self {
        Response {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}

/// Convert an API result into a warp response.
///
/// Should be used in conjunction with `handle_rejection`.
///
/// # Example
///
/// ```
/// use chainflip::common::api::{respond, ResponseError, handle_rejection};
/// use warp::Filter;
/// use std::future::Future;
///
/// async fn hello_world() -> Result<String, ResponseError> {
///     Ok("Hello world".to_owned())
/// }
///
/// async fn return_error() -> Result<String, ResponseError> {
///     Err(ResponseError::new(warp::http::StatusCode::NOT_FOUND, "Page not found"))
/// }
///
/// let example_route = warp::get()
///     .and(warp::path("example"))
///     .map(hello_world)
///     .and_then(respond);
///
/// let error_route = warp::get()
///     .and(warp::path("error"))
///     .map(return_error)
///     .and_then(respond);
///
/// let routes = example_route
///     .or(error_route)
///     .recover(handle_rejection);
///
/// warp::serve(routes).run(([127, 0, 0, 1], 3030));
/// ```
pub async fn respond<T, F>(result: F) -> Result<impl warp::Reply, warp::Rejection>
where
    T: Serialize,
    F: Future<Output = Result<T, ResponseError>>,
{
    let result = result.await;
    match result {
        Ok(data) => {
            let response = Response::success(data);
            Ok(warp::reply::with_status(
                warp::reply::json(&response),
                warp::http::StatusCode::OK,
            ))
        }
        Err(error) => Err(warp::reject::custom(error)),
    }
}

/// Warp custom rejection handler.
///
/// Should be used in conjunction with `respond`.
///
/// # Example
///
/// ```
/// use chainflip::common::api::{respond, ResponseError, handle_rejection};
/// use warp::Filter;
/// use std::future::Future;
///
/// async fn hello_world() -> Result<String, ResponseError> {
///     Ok("Hello world".to_owned())
/// }
///
/// async fn return_error() -> Result<String, ResponseError> {
///     Err(ResponseError::new(warp::http::StatusCode::NOT_FOUND, "Page not found"))
/// }
///
/// let example_route = warp::get()
///     .and(warp::path("example"))
///     .map(hello_world)
///     .and_then(respond);
///
/// let error_route = warp::get()
///     .and(warp::path("error"))
///     .map(return_error)
///     .and_then(respond);
///
/// let routes = example_route
///     .or(error_route)
///     .recover(handle_rejection);
///
/// warp::serve(routes).run(([127, 0, 0, 1], 3030));
/// ```
pub async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
    let response_error;

    if err.is_not_found() {
        response_error = ResponseError::new(warp::http::StatusCode::NOT_FOUND, "Not Found");
    } else if let Some(error) = err.find::<ResponseError>() {
        response_error = error.clone();
    } else if let Some(_) = err.find::<warp::filters::body::BodyDeserializeError>() {
        response_error = ResponseError::new(warp::http::StatusCode::BAD_REQUEST, "Invalid Body");
    } else if let Some(_) = err.find::<warp::reject::InvalidQuery>() {
        response_error = ResponseError::new(warp::http::StatusCode::BAD_REQUEST, "Invalid Query");
    } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
        response_error = ResponseError::new(
            warp::http::StatusCode::METHOD_NOT_ALLOWED,
            "Method Not Allowed",
        );
    } else {
        // In case we missed something - log and respond with 500
        error!("unhandled rejection: {:?}", err);
        response_error = ResponseError::new(
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong",
        );
    }

    let status = warp::http::StatusCode::from_u16(response_error.code)
        .expect("Expected a valid HTTP status code");
    let response = Response::<()>::failure(response_error);
    let json = warp::reply::json(&response);

    Ok(warp::reply::with_status(json, status))
}

/// Use a custom param in warp
///
/// # Example
///
/// ```
/// use chainflip::common::api::{respond, ResponseError, handle_rejection, using};
/// use warp::Filter;
/// use std::sync::Arc;
///
/// async fn hello(string: Arc<String>, another_string: String) -> Result<String, ResponseError> {
///     Ok(format!("Hello {}{}", string, another_string))
/// }
///
/// let string = Arc::new(String::from("world"));
///
/// let hello = warp::get()
///     .and(warp::path("hello"))
///     .and(using(string.clone()))
///     .and(using(String::from("!")))
///     .map(hello)
///     .and_then(respond)
///     .recover(handle_rejection);
///
/// warp::serve(hello).run(([127, 0, 0, 1], 3030));
/// ```
pub fn using<S>(param: S) -> impl Filter<Extract = (S,), Error = std::convert::Infallible> + Clone
where
    S: Clone + Send,
{
    warp::any().map(move || param.clone())
}
