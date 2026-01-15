use super::String;
pub type BrokerInfo = super::before_version_15::BrokerInfo<String>;

impl From<super::before_version_3::BrokerInfo> for BrokerInfo {
	fn from(old: super::before_version_3::BrokerInfo) -> Self {
		Self { earned_fees: old.earned_fees, ..Default::default() }
	}
}
