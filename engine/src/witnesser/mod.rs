//! Common Witnesser functionality

pub mod block_head_stream_from;

pub trait BlockNumberable {
	fn block_number(&self) -> u64;
}
