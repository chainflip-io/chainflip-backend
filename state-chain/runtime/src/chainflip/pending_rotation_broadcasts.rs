use cf_traits::RotationBroadcastsPending;

#[macro_export]
macro_rules! cons_rotation_broadcasts_pending {
    ($last: ty) => {
        $last
    };

    ($head: ty, $($tail:ty),+) => {
        $crate::chainflip::pending_rotation_broadcasts::ConsRotationBroadcastsPending<$head, cons_rotation_broadcasts_pending!($($tail),+)>
    }
}

pub struct ConsRotationBroadcastsPending<H, T>(H, T);

impl<H, T> RotationBroadcastsPending for ConsRotationBroadcastsPending<H, T>
where
	H: RotationBroadcastsPending,
	T: RotationBroadcastsPending,
{
	fn rotation_broadcasts_pending() -> bool {
		H::rotation_broadcasts_pending() || T::rotation_broadcasts_pending()
	}
}
