This crate implements a derive macro that allows us to work around a bug in the Javascript and Python libraries implementing SCALE decoding for Substrate RPC calls.
Usually, for types that need to be represented in SCALE, we use `#[derive(TypeInfo)]`, which is a derive macro provided by Parity. This will
automatically implement the `TypeInfo` trait, which is what ultimately generates the type metadata used by Substrate. The problem arises because we
have generic pallets, which contain types that depend on the generic type parameter. Even if the names of these generic types are the same, their types may not be. This
confuses the SCALE libraries, because they rely on the name of the type to resolve it in the type metadata (they should really use the uniqe type ID, but they don't).
In this crate, we provide an alternative macro `#[derive(GenericTypeInfo)]`. It can be used on types that are generic and contain generic types. The requirement is that
the first generic parameter contains a static `NAME` element. For example:

```
#[derive(GenericTypeInfo)]
pub struct MyStruct<T: Config<I>, I: 'static = ()> {
	pub foo: u32,
	pub bar: SpecialType<T, I>,
}```

Here, `MyStruct` is generic and contains a generic type `SpecialType`. Depending on `T` and `I`, the actual type of `SpecialType` may be different, but the typename will always be `"SpecialType<T, I>"`, because derive macros are executed before generics are expanded. Thus, the current SCALE libraries cannot uniquely determine the correct type
based on the type name alone and will usually just fail. Using `#[derive(GenericTypeInfo)]` here will instead assume that `T::NAME` is a static string uniquely determining
the type `T` (for example "Bitcoin") and will generate type names like `SpecialType<T, I>forBitcoin` instead.