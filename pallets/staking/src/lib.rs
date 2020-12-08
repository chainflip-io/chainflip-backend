// #![cfg_attr(not(feature = "std"), no_std)]

// use frame_support::{decl_module, decl_storage, decl_event, decl_error};
// use frame_system::ensure_signed;

// // 2. Configuration
// pub trait Trait: frame_system::Trait { 
//     type Event: From<Event<Self>> + Into<<Self as frame_system::Trait>::Event>;
// }

// // Are we adding anything to storage here? or just calling Aura / Grandpa's APIs?
// // decl_storage! { 
// //     todo!();
// // }

// // 4. Events
// decl_event! {
//     pub enum Event<T> where AccountId = <T as frame_system::Trait>::AccountId {
//         // [who]
//         StakeInitiated(AccountId),
//         // [who]
//         StakerAdded(AccountId),
//         // [who]
//         UnstakeInitiated(AccountId),
//         // [who]
//         StakerRemoved(AccountId),
//     }
// }

// // 5. Errors
// decl_error! {
//     pub enum Error for Module<T: Trait> {
//         // account is not a validator, so can't be removed
//         NotValidator,
//         // account is already a validator, can't become a validator again
//         AlreadyValidator,
//     }
// }

// // 6. Callable Functions
// decl_module! { 
//     // Errors must be initialized if they are used by the pallet.
//     type Error = Error<T>;

//     // Events must be initialized if they are used by the pallet.
//     fn deposit_event() = default;

//     /// Allow a user to claim ownership of an unclaimed proof.
//     #[weight = 0]
//     fn create_claim(origin, proof: Vec<u8>) {
//         todo!();
//     }
// }