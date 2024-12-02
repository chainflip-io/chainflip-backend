pub mod __macro_imports {
	pub use heck::{AsLowerCamelCase, ToLowerCamelCase, ToSnakeCase};
	pub use schemars::{generate::SchemaSettings, JsonSchema, Schema, SchemaGenerator};
	pub use serde::{Deserialize, Serialize};
	pub use serde_with::{DeserializeFromStr, SerializeDisplay};
}

/// Implements a schema generator for the specified endpoints.
///
/// An 'endpoint' is the definition of a request/response pair for a specific API call.
///
/// Use this macro to bootstrap the structure for generating an API that supports JSON Schema.
///
/// This macro will generate a `schema` module the defines a Schema endpoint for returning the JSON
/// Schema for each of the defined endpoints (the `Schema` endpoint can be included in this).
///
/// The `ApiWrapper` struct is also generated, which is used to wrap the API that is being exposed.
/// Wrapping is necessary to allow implementation of the Responder trait on foreign API types.
///
/// To implement a new endpoint:
///
/// 1. Define a module for the endpoint.
/// 2. Define the request and response types for the endpoint.
/// 3. Implement the `Endpoint` trait for the endpoint.
/// 4. Implement the `Responder` trait for the `ApiWrapper` struct.
/// 5. Implement the `ArrayParam` trait for the request type. This trait is available from the
///    `jsonrpsee_flatten` crate and needs to be included in the Cargo.toml of the Api
///    implementation.
/// 6. Add the endpoint to the `impl_schema_endpoint!` macro.
///
/// # Example
///
/// ```ignore
/// // Create a module for each endpoint.
/// mod my_endpoint {
///     //////
///     // Define (or import) the request and response types.
///     // These types should derive or implement Debug, Clone, Serialize, Deserialize, and JsonSchema.
///     //////
///     #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
///     pub struct MyRequest {
///         a: String,
///         b: bool,
///     }
///
///     //////
///     // Even for simple types, prefer to use a named struct and add the necessary traits and documentation.
///     //////
///
///     /// My simple response type.
///     ///
///     /// This is a simple response type that just contains an integer representing blah blah.
///     #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
///     pub struct MyResponse(i32);
///
///     //////
///     // The request must implement the `ArrayParam`` trait.
///     // The jsonrpsee_flatten crate is a fork of jsonrpsee, defined in our workspace Cargo.toml.
///     // It defines a conversion from a flattened tuple representation to a named struct and back.
///     //////
///
///     impl jsonrpsee_flatten::types::ArrayParam for MyRequest {
///         type ArrayTuple = (String,bool);
///
///         fn into_array_tuple(self) -> Self::ArrayTuple {
///            (self.a, self.b)
///         }
///         fn from_array_tuple((a,b): Self::ArrayTuple) -> Self {
///             Self { a, b }
///         }
///     }
///
///     //////
///     // Define a struct that implements the Endpoint trait.
///     //////
///
///     pub struct Endpoint;
///     impl api_json_schema::Endpoint for Endpoint {
///         type Request = MyRequest;
///         type Response = MyResponse;
///         type Error = anyhow::Error;
///     }
///
///     //////
///     // Implement the responder for the API wrapper.
///     //////
///
///     impl<T: SomeApi> api_json_schema::Responder<Endpoint> for ApiWrapper<T> {
///         async fn respond(
///            &self,
///            MyRequest { a, b }: MyRequest,
///         ) -> api_json_schema::EndpointResult<Endpoint> {
///            Ok(a.len() as i32)
///         }
///     }
/// }
///
/// //////
/// // Include the endpoint in the schema.
/// //////
///
/// api_json_schema::impl_schema_endpoint! {
///    prefix: "my_api_",
///    MyEndpoint: my_endpoint::Endpoint,
///    // The generated `Schema` endpoint can be included in the list of documented endpoints.
///    Schema: schema::Endpoint,
/// }
///
/// //////
/// // The generated `ApiWrapper` struct can be used to wrap the API that is being exposed.
/// //////
///
/// async fn print_schema() {
///     use serde_json;
///
///     // Default request is empty, meaning all methods are included.
///     let request = schema::SchemaRequest::default();
///     let schema = api_json_schema::respond::<_, schema::Endpoint>(
///         SchemaApi,
///         Default::default(),
///     ).await;
///     println!("{}", serde_json::to_string_pretty(&schema).unwrap());
/// }
///
/// async fn process_my_request(request: MyRequest) -> MyResponse {
///     api_json_schema::respond::<_, my_endpoint::Endpoint>(
///         ApiWrapper { api: SomeApi::new() },
///         request,
///     ).await
/// }
/// ```
#[macro_export]
macro_rules! impl_schema_endpoint {
	(
		prefix: $prefix:literal,
		$(
			$method_name:ident: $endpoint:ty
		),+ $(,)?
	) => {
		#[derive(Debug, Clone, Copy)]
		pub struct ApiWrapper<T> {
			/// For accessing the wrapped API.
			pub api: T,
		}

		impl<T> core::ops::Deref for ApiWrapper<T> {
			type Target = T;

			fn deref(&self) -> &Self::Target {
				&self.api
			}
		}
		impl<T: Default> Default for ApiWrapper<T> {
			fn default() -> Self {
				ApiWrapper {
					api: Default::default()
				}
			}
		}

		pub mod schema {
			use $crate::schema_macro::__macro_imports::*;
			use super::*;

			pub struct Endpoint;

			#[derive(Debug, PartialEq, Eq, Clone, SerializeDisplay, DeserializeFromStr)]
			pub enum Method {
				$(
					$method_name,
				)+
			}

			impl JsonSchema for Method {
				fn schema_name() -> std::borrow::Cow<'static, str> {
					"Method".into()
				}

				fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
					json_schema!({
						"type": "string",
						"enum": Self::all().iter().map(|m| m.to_string()).collect::<Vec<_>>()
					})
				}
			}

			impl Method {
				const fn all() -> &'static [Self] {
					&[
						$(
							Method::$method_name,
						)+
					]
				}
			}

			#[derive(Debug, Default, PartialEq, Eq, Clone, JsonSchema, Serialize, Deserialize)]
			pub struct SchemaRequest {
				#[serde(skip_serializing_if = "Vec::is_empty")]
				#[serde(default)]
				methods: Vec<Method>,
			}

			impl core::fmt::Display for Method {
				fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
					write!(f, "{}{}", $prefix, match self {
						$(
							Method::$method_name => AsLowerCamelCase(stringify!($method_name)),
						)+
					})
				}
			}

			impl std::str::FromStr for Method {
				type Err = String;

				fn from_str(s: &str) -> Result<Self, Self::Err> {
					let method = s.strip_prefix($prefix)
						.ok_or_else(|| format!("Invalid prefix. Must be {}", $prefix))?
						.to_string();

					$(
						if [
							stringify!($method_name).to_lower_camel_case(),
							stringify!($method_name).to_snake_case(),
						].contains(&method) {
							return Ok(Method::$method_name)
						}
					)+

					Err(format!("Unknown method: {}", method))
				}
			}

			#[derive(Debug, PartialEq, Clone, JsonSchema, Serialize, Deserialize)]
			pub struct Response {
				pub methods: Vec<EndpointSchema>,
			}

			#[derive(Debug, PartialEq, Clone, JsonSchema, Serialize, Deserialize)]
			pub struct EndpointSchema {
				method: Method,
				request: Schema,
				response: Schema,
			}

			#[derive(Debug, PartialEq, Clone, JsonSchema, Serialize, Deserialize)]
			pub struct SchemaDefs {
				pub request: serde_json::Map<String, serde_json::Value>,
				pub response: serde_json::Map<String, serde_json::Value>,
			}

			impl $crate::Endpoint for Endpoint {
				type Request = SchemaRequest;
				type Response = Response;
				type Error = $crate::Never;
			}

			pub struct SchemaApi;

			impl $crate::Responder<Endpoint> for SchemaApi {
				async fn respond(
					&self,
					request: $crate::EndpointRequest<Endpoint>
				) -> $crate::EndpointResult<Endpoint> {
					// Assume that callers want to know how to serialize a request and deserialize a response.
					let mut ser_generator = SchemaSettings::default()
						.with(|settings| {
							settings.option_add_null_type = false;
							settings.inline_subschemas = true;
						})
						.for_serialize()
						.into_generator();
					let mut de_generator = SchemaSettings::default()
						.with(|settings| {
							settings.option_add_null_type = false;
							settings.inline_subschemas = true;
						})
						.for_deserialize()
						.into_generator();

					let methods = if request.methods.is_empty() {
							Method::all().to_vec()
						} else {
							request.methods
						}
						.into_iter()
						.map(|method| {
							match method {
								$(
									Method::$method_name => EndpointSchema {
										method,
										request: ser_generator
											.subschema_for::<$crate::EndpointRequest<$endpoint>>(),
										response: de_generator
											.subschema_for::<$crate::EndpointResponse<$endpoint>>(),
									},
								)+
							}
						})
						.collect::<Vec<_>>();

					Ok(Response {
						methods,
					})
				}
			}

			impl jsonrpsee_flatten::types::ArrayParam for SchemaRequest {
				type ArrayTuple = (Vec<Method>,);

				fn into_array_tuple(self) -> Self::ArrayTuple {
					(self.methods.clone(),)
				}

				fn from_array_tuple((methods,): Self::ArrayTuple) -> Self {
					Self { methods }
				}
			}
		}
	}
}
