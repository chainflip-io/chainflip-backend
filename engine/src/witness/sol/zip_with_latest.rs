use std::{
	marker::PhantomData,
	pin::Pin,
	task::{Context, Poll},
};

use futures::{Stream, TryStream};

pub trait ZipLatestExt: Stream + Sized {
	fn zip_latest<Right>(self, right: Right) -> ZipLatest<Self, Right, Right::Item, Tuple>
	where
		Right: Stream + Sized,
	{
		ZipLatest::new(self, right)
	}
}

pub trait TryZipLatestExt:
	TryStream + Stream<Item = Result<Self::Ok, Self::Error>> + Sized
{
	fn try_zip_latest<Right>(
		self,
		right: Right,
	) -> ZipLatest<Self, Right, Right::Item, TryLeftThenTuple>
	where
		Right: Stream + Sized,
	{
		ZipLatest::new(self, right)
	}
}

pub struct Tuple;
pub struct TryLeftThenTuple;

#[derive(Debug, Clone)]
#[pin_project::pin_project]
pub struct ZipLatest<Left, Right, RightItem, With> {
	#[pin]
	left: Left,
	#[pin]
	right: Right,

	last_right: Option<RightItem>,

	_pd: PhantomData<With>,
}

pub trait ZipWith<L, R> {
	type Out;

	fn combine(left: L, right: R) -> Self::Out;
}

impl<L, R, W> ZipLatest<L, R, R::Item, W>
where
	L: Stream + Sized,
	R: Stream + Sized,
	W: ZipWith<L::Item, R::Item>,
{
	pub fn new(left: L, right: R) -> Self {
		Self { left, right, last_right: None, _pd: Default::default() }
	}
}

impl<L, R> ZipWith<L, R> for Tuple {
	type Out = (L, R);
	fn combine(left: L, right: R) -> Self::Out {
		(left, right)
	}
}

impl<L, E, R> ZipWith<Result<L, E>, R> for TryLeftThenTuple {
	type Out = Result<(L, R), E>;

	fn combine(left: Result<L, E>, right: R) -> Self::Out {
		left.map(move |left| (left, right))
	}
}

impl<L, R, W> Stream for ZipLatest<L, R, R::Item, W>
where
	L: Stream,
	R: Stream,
	W: ZipWith<L::Item, R::Item>,
	R::Item: Clone,
{
	type Item = W::Out;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		use std::task::ready;

		let mut this = self.project();
		Poll::Ready(loop {
			if let Some(last_right) = this.last_right.as_mut() {
				let Some(another_left) = ready!(this.left.as_mut().poll_next(cx)) else {
					break None
				};

				break loop {
					match this.right.as_mut().poll_next(cx) {
						Poll::Pending => break Some(W::combine(another_left, last_right.clone())),
						Poll::Ready(None) => break None,
						Poll::Ready(Some(newer_right)) => *last_right = newer_right,
					}
				}
			} else {
				let Some(first_right) = ready!(this.right.as_mut().poll_next(cx)) else {
					break None
				};
				*this.last_right = Some(first_right);
			}
		})
	}
}

impl<S> ZipLatestExt for S where S: Stream + Sized {}
impl<S> TryZipLatestExt for S where
	S: TryStream + Stream<Item = Result<Self::Ok, Self::Error>> + Sized
{
}
