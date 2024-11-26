#[macro_export]
macro_rules! impl_schema_endpoint {
	($( $mod:ident: $endpoint:ident ),+ $(,)? ) => {
		pub mod schema {
			use $crate::api::{self, *};

			pub struct Endpoint;
			pub struct Responder;

			#[derive(Debug, PartialEq, Eq, Clone, JsonSchema, serde_with::SerializeDisplay, serde_with::DeserializeFromStr)]
			#[serde(rename_all = "snake_case")]
			pub enum Method {
				$(
					$endpoint,
				)+
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

			impl api::Endpoint for Endpoint {
				type Request = Vec<Method>;
				type Response = Response;
				type Error = Never;
			}

			#[async_trait]
			impl api::Responder<Endpoint> for Responder {
				async fn respond(
					&self,
					request: api::EndpointRequest<Endpoint>
				) -> api::EndpointResult<Endpoint> {
					use schemars::generate::SchemaSettings;

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

					let methods = if request.is_empty() {
						vec![
							$(
								Method::$endpoint,
							)+
						].into_iter()
					} else {
						request.into_iter()
					}
					.map(|method| {
						match method {
							$(
								Method::$endpoint => EndpointSchema {
									method,
									request: ser_generator
										.subschema_for::<api::EndpointRequest<api::$mod::Endpoint>>(),
									response: de_generator
										.subschema_for::<api::EndpointResponse<api::$mod::Endpoint>>(),
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
		}
	}
}
