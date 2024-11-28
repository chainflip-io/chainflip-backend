pub mod __macro_imports {
	pub use schemars::{generate::SchemaSettings, JsonSchema, Schema, SchemaGenerator};
	pub use serde::{Deserialize, Serialize};
	pub use serde_with::{DeserializeFromStr, SerializeDisplay};
}

#[macro_export]
macro_rules! impl_schema_endpoint {
	($( $mod:ident: $endpoint:ident ),+ $(,)? ) => {
		pub mod schema {
			use $crate::schema_macro::__macro_imports::*;
			use super::*;

			pub struct Endpoint;
			pub struct Responder;

			#[derive(Debug, PartialEq, Eq, Clone, JsonSchema, SerializeDisplay, DeserializeFromStr)]
			#[serde(rename_all = "snake_case")]
			pub enum Method {
				$(
					$endpoint,
				)+
			}

			#[derive(Debug, Default, PartialEq, Eq, Clone, JsonSchema, Serialize, Deserialize)]
			pub struct SchemaRequest {
				#[serde(skip_serializing_if = "Vec::is_empty")]
				#[serde(default)]
				methods: Vec<Method>,
			}

			impl core::fmt::Display for Method {
				fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
					write!(f, "{}", match self {
						$(
							Method::$endpoint => concat!("broker_", stringify!($endpoint)),
						)+
					})
				}
			}

			impl std::str::FromStr for Method {
				type Err = String;

				fn from_str(s: &str) -> Result<Self, Self::Err> {
					match s {
						$(
							concat!("broker_", stringify!($endpoint)) => Ok(Method::$endpoint),
						)+
						_ => Err(format!("Unknown method: {}", s)),
					}
				}
			}

			#[derive(Debug, PartialEq, Clone, JsonSchema, Serialize, Deserialize)]
			pub struct Response {
				pub methods: Vec<EndpointSchema>,
				#[serde(rename = "$defs")]
				pub defs: SchemaDefs,
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

			impl $crate::Responder<Endpoint> for Responder {
				async fn respond(
					&self,
					request: $crate::EndpointRequest<Endpoint>
				) -> $crate::EndpointResult<Endpoint> {
					// Assume that callers want to know how to serialize a request and deserialize a response.
					let mut ser_generator = SchemaSettings::default()
						.with(|settings| {
							settings.definitions_path = String::from("#/$defs/request");
							settings.option_add_null_type = false;
						})
						.for_serialize()
						.into_generator();
					let mut de_generator = SchemaSettings::default()
						.with(|settings| {
							settings.definitions_path = String::from("#/$defs/response");
							settings.option_add_null_type = false;
						})
						.for_deserialize()
						.into_generator();

					let methods = if request.methods.is_empty() {
						vec![
							$(
								Method::$endpoint,
							)+
						].into_iter()
					} else {
						request.methods.into_iter()
					}
					.map(|method| {
						match method {
							$(
								Method::$endpoint => EndpointSchema {
									method,
									request: ser_generator
										.subschema_for::<$crate::EndpointRequest<$mod::Endpoint>>(),
									response: de_generator
										.subschema_for::<$crate::EndpointResponse<$mod::Endpoint>>(),
								},
							)+
						}
					})
					.collect::<Vec<_>>();

					Ok(Response {
						methods,
						defs: SchemaDefs {
							request: ser_generator.take_definitions(),
							response: de_generator.take_definitions(),
						}
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
