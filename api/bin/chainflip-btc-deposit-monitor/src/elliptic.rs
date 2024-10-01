
use std::{env, fmt::format, time::{SystemTime, UNIX_EPOCH}};

use chainflip_api::primitives::state_chain_runtime::Header;
use reqwest::{header::{self, HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE}, Client, Url};
use serde_json::json;
use anyhow::{anyhow, Result};
use base64::prelude::*;
use chrono::prelude::*;
use std::fs;

use sha2::Sha256;
use hmac::{Hmac, Mac};
// use hex_literal::hex;

// Create alias for HMAC-SHA256
type HmacSha256 = Hmac<Sha256>;


const SINGLE_ANALYSIS_URL: &str = "https://aml-api.elliptic.co/v2/analyses/synchronous";

pub struct EllipticClient {
	client: Client,
}

pub struct EllipticAnalysisResult {
	pub risk_score: f64
}

impl EllipticClient {
	pub fn new() -> Self {
		EllipticClient {
			client: Client::new()
		}
	}

	fn get_signature(secret: String, time_of_request: u128, http_method: String, http_path: String, payload: String) -> String {
		// // create a SHA256 HMAC using the supplied secret, decoded from base64
		// const hmac = crypto.createHmac('sha256', Buffer.from(secret, 'base64'));
		let secret = BASE64_STANDARD.decode(secret).expect("could not decode");
		let mut hmac = HmacSha256::new_from_slice(&secret).expect("hmac failed");

		// // concatenate the request text to be signed
		// const request_text = time_of_request + http_method + http_path.toLowerCase() + payload;
		let request_text = time_of_request.to_string() + &http_method + &http_path.to_ascii_lowercase() + &payload;

		// // update the HMAC with the text to be signed
		// hmac.update(request_text);
		hmac.update(request_text.as_bytes());

		// // output the signature as a base64 encoded string
		// return hmac.digest('base64');
		let result = hmac.finalize();

		BASE64_STANDARD.encode(result.into_bytes())
	}

	pub async fn single_analysis(&self, hash: String, output_address: String, customer_reference: String) -> Result<f64> {
		let request_body = json!({
			"subject": {
				"type": "transaction",
				"output_type": "address",
				"asset": "BTC",
				"blockchain": "bitcoin",
				"hash": hash,
				"output_address": output_address,
			},
			"type": "source_of_funds",
			"customer_reference": customer_reference
		});

		let access_key = env::var("ELLIPTIC_ACCESS_KEY").expect("access key not set");
		let access_timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
		let access_sign = Self::get_signature(env::var("ELLIPTIC_SECRET_KEY").expect("secret key not set"), access_timestamp, "POST".into(), "/v2/analyses/synchronous".into(), request_body.to_string());
		
		let mut headers = HeaderMap::new();
		headers.insert(CONTENT_TYPE, "application/json".parse().unwrap());
		headers.insert(ACCEPT, "application/json".parse().unwrap());
		headers.insert("x-access-key", access_key.parse().unwrap());
		headers.insert("x-access-sign", access_sign.parse().unwrap());
		headers.insert("x-access-timestamp", access_timestamp.to_string().parse().unwrap());

		let result = self.client.post(SINGLE_ANALYSIS_URL)
			.json(&request_body)
			.headers(headers)
			.send()
			.await
			.map_err(|e| anyhow!("error in transport: {e}"))?;

		let res : serde_json::Value = result.json().await?;
		// match result.json::<serde_json::Value>().await {
		// 	Ok(x) => println!("Got result: {x}"),
		// 	Err(e) => println!("Error: {e}")
		// }
		// res.

		let filename = format!("elliptic_responses/{}.json", Utc::now());

		fs::write(filename, res.to_string()).expect("Unable to write file");


		let risk_score = res.as_object()
			.ok_or(anyhow!("Unexpected format from elliptic"))?
			.get("risk_score")
			.ok_or(anyhow!("elliptic response didn't contain a risk_score"))?
			.as_f64()
			.ok_or(anyhow!("risk_score is not a float!"))?;

		Ok(risk_score)
	}

	pub async fn welltyped_single_analysis(&self, hash: bitcoin::Txid, output_address: bitcoin::Address, customer_reference: String) -> Result<EllipticAnalysisResult> {
		self.single_analysis(hash.to_string(), output_address.to_string(), customer_reference)
			.await
			.map(|risk_score| EllipticAnalysisResult {risk_score})
	}
}