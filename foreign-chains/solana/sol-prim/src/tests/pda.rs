#![cfg(feature = "pda")]

use crate::{
	address::Address,
	pda::{Pda, PdaError},
};

mod failures {
	use crate::consts;

	use super::*;

	#[test]
	fn seed_too_long() {
		let public_key: Address =
			"J4mK4RXAuizk5aMZw8Vz8W3y7mrCy6dcgniZ4qwZimZE".parse().expect("public key");
		assert!(matches!(
			Pda::from_address(public_key)
				.expect("derive")
				.chain_seed("01234567890123456789012345678912")
				.expect("32 should be still okay")
				.chain_seed("012345678901234567890123456789123")
				.expect_err("33 should be too much"),
			PdaError::SeedTooLarge
		));
	}

	#[test]
	fn too_many_seeds() {
		let public_key: Address =
			"J4mK4RXAuizk5aMZw8Vz8W3y7mrCy6dcgniZ4qwZimZE".parse().expect("public key");
		(1..consts::SOLANA_PDA_MAX_SEEDS)
			.map(|i| [i])
			.try_fold(Pda::from_address(public_key).expect("derive"), Pda::chain_seed)
			.expect("15 should be okay");
		assert!(matches!(
			(1..=consts::SOLANA_PDA_MAX_SEEDS)
				.map(|i| [i])
				.try_fold(Pda::from_address(public_key).expect("derive"), Pda::chain_seed)
				.expect_err("16 should be too many"),
			PdaError::TooManySeeds
		));
	}

	#[test]
	fn initial_address_should_be_a_valid_point() {
		let public_key: Address =
			"J4mK4RXAuizk5aMZw8Vz8W3y7mrCy6dcgniZ4qwZimZE".parse().expect("public key");
		let (pda, _bump) = Pda::from_address(public_key).expect("derive").finish().expect("finish");
		assert!(matches!(
			Pda::from_address(pda).expect_err("PDA can't be a valid point on a curve"),
			PdaError::NotAValidPoint,
		))
	}
}

mod happy {
	use super::*;
	fn run_single(public_key: &str, seeds: &[&str], expected_pda: &str) {
		let public_key: Address = public_key.parse().expect("public-key");
		let expected_pda: Address = expected_pda.parse().expect("expected-pda");
		let (actual_pda, _) = seeds
			.into_iter()
			.try_fold(Pda::from_address(public_key).expect("derive"), Pda::chain_seed)
			.expect("chain-seed")
			.finish()
			.expect("finish");
		assert_eq!(actual_pda, expected_pda);
	}

	fn run_multiple(public_key: &str, seeds: &[&str], expected: &[&str]) {
		for (i, exp) in expected.iter().copied().enumerate() {
			run_single(public_key, &seeds[0..i], exp);
		}
	}

	#[test]
	fn t_01() {
		run_multiple(
			"J4mK4RXAuizk5aMZw8Vz8W3y7mrCy6dcgniZ4qwZimZE",
			&["01", "02", "03", "04", "05", "06", "07", "08", "09", "10"],
			&[
				"5y4ZsPDKAXv4FTmK7C4BVhRRcqhrHYhUfYNhj1nS2EJR",
				"26ytLSPyng5vEbiU5oheMWjFcnsqjZ7pDZh7VeY3opVA",
				"56v8wrZ3XnVEDKBNf61wXGSZUytG74HL15U6QKSBPcfs",
				"7JmpBCpuk2C6URzk5sef2QEGkUKaYoWFVzd6VqnMphW7",
				"DHiL65LHzm6vEHqg7QdWQrAeDccFERY7ncWQQRN8eMZ2",
				"JBJhHAdFde2DEBu3BC7PvjP3gKccJdkYsZ5tuZJZ662Z",
				"966sm2bMX53KShTPyi7wWSS2CoL1KF2wcq8bfVPXp2k5",
				"GmVkvb711u4cxWu4cgL7BfQVavkvAgAosetYNozj18im",
				"G9JD6sCCEuGGoxaS6vtnANaHgoHaRZEAya3J3bG9CA6i",
				"AmKZXFmsD4RYBKmbRm1fnf4hNtgUrbLg24hyhcVdR5Cj",
				"FGB8eaQ6ftaTxAuDgajdfGr2Zz3UYB2L7N8AdRm1LLj8",
			],
		);
	}

	#[test]
	fn t_02() {
		run_multiple(
			"CMvtEhZFrNckPbBBAMG9H5vWQKgRhizUpa1zgsocHitt",
			&["01", "02", "03", "04", "05", "06", "07", "08", "09", "10"],
			&[
				"EPxbG1EAA9v9Q8jdB2WG16dp85Rf7rwqTTxRxYusVWNL",
				"DwdcNhRxquwPkaCr6MLqv3s51uRCDqLjjyeW7ktMXy3F",
				"Ebd9J8T3JcrzteYEUHHn3qAhoDqKcZKhjzFrbHy6Qqf6",
				"8hkpx1KbwQdekFX8kwpCxE3Z27qeFEado73mg3Ex3nUN",
				"GxuJjgtE1UKrsWw3TYaC77LA4VTXYrhxhq3TfYUtWLvc",
				"1SugNChXjhwd7qVoyzAnEvEL5bTrKF2sQAukBeFxHCi",
				"E6E3GNnQ1PgDWGPJcptLBF8enNBpoo9Q2aYQ5NGc7EMB",
				"9KBiqr8QRHP3P8zwPTgyXsguySgK14MQdtcjx5Sx8XKq",
				"Fo72PPK2Uwmg8V6sRBkTrjyEcLEAa7N9gUrGm4SeJdJV",
				"FAAoXyjPcSFrtHQW2b7ca6nbrjcYAMGkYRWb7nr2iXiE",
				"6KvRtazxmARA6YPwwoTVd3ZoVXi4VPAffGBhJmSmRxVg",
			],
		);
	}

	#[test]
	fn t_03() {
		run_multiple(
			"BNbigAb1hATnMEN9N8sXp935SwC7FMQSNBrxpB8QQrWH",
			&["01", "02", "03", "04", "05", "06", "07", "08", "09", "10"],
			&[
				"CwDHrktLSbaVMGqgFNrKvS512mcKjrkEr3QGHo6fkRGg",
				"2aGGWpCQeSyEwsPqBKfa2teY79EZp8LhiNYaV7Ccnmct",
				"4G46zqeidyGD1NJ5Pze6maPqGvjYcokExQHKv3YPaQvr",
				"Aqf7ZwkXwQuWCMWxKHyhULRC7dUFMa9dNHZFbyNckd1V",
				"ExBQ1rCH7Dvq4cPP5svrANRZq11oQbcKydWZCxUhiPdz",
				"CAAdBuFsbZC1Et9xtqVXr1TJzoxv54DUpA8ZFNPQxZCE",
				"Af9jnbhXBqb5sbLJpuAxvVM6RJSdftaUobaNvgn2YLgT",
				"CSmzwXBcNGp8AvLYEwsd2EbGXt6NidcGcYoQi8GaGuza",
				"81EfK2rHskzFB6MAMwzR9inzvSL4oidnTF5inih5JNqW",
				"Fxzp8DYkpPus7tgoaM6vMtNcJXTEW3jCxKyYJGvRdEt2",
				"J649X8yD9UDKLPEiRPzgm61fuDE94PNhRGi6y2hqWKCB",
			],
		);
	}

	#[test]
	fn t_04() {
		run_multiple(
			"DUz6zmxp17qetu9Zpmrs69Hk6k5QyyvCnjqpuv72KXzw",
			&["01", "02", "03", "04", "05", "06", "07", "08", "09", "10"],
			&[
				"D2RsmKxT8qfp2aFSJo4uraBthx68m6TCrRiZpJgdCzMF",
				"EUcVypqKucAUCHswWCRY4Por5r1X2ZAEta7S1twfFaQ8",
				"Cvhh6HWGHek1jqH3rq8z5JL9d5ZCj8BwzJeQcsLUpvBw",
				"GkmcJ3zcYpmaXfM1aPab42DrKA8BY4wZygYZAxDspiR2",
				"C2t5TpVkUFxAwYEzakuV8gV2tLa2n3sCK37qHJxXX88t",
				"J3DV1SWHkFPJyjF2QSCnYF1oLDBfV8ckfN6wHrXtBaY5",
				"2stcNqugnopgUoDGhbneaM1Gt5RdRcPGSq7jAhpgmg1s",
				"4ywmo982EVzBpoEhb3QcjF4iqtmApy4AvXG53AFsZCEL",
				"BPHSCEc4GscoPYv4LKCBKPBvehRyvis33KDMmN1KiJ5k",
				"25S1H7qCJNhXwTWZifgEv7e6odAQSK9SwtWZeB1LvRQk",
				"FnWzpNpUmjnjidJFiU89WFHvDL9UHzSnNgbJvvMmH6oi",
			],
		);
	}
}
