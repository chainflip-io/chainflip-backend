#![cfg(test)]
use generic_typeinfo_derive::GenericTypeInfo;
use scale_info::{TypeDef, TypeInfo};

trait NameTrait {
	const SOME_NAME: &'static str;
}

// TypeInfo will not be implemented "recursively"
// Therefore, if we want a field of type MyStruct later,
// we also need to implement TypeInfo on MyStruct
#[derive(TypeInfo)]
struct MyStruct;

impl NameTrait for MyStruct {
	const SOME_NAME: &'static str = "Example";
}

#[derive(GenericTypeInfo)]
#[expand_name_with(T::SOME_NAME)]
struct Foo<T: 'static + NameTrait + TypeInfo> {
	_hewey: T,
	#[skip_name_expansion]
	_dewey: u16,
	#[replace_typename_with(MuchCoolerTypename)]
	_louis: T,
}

#[derive(GenericTypeInfo)]
#[expand_name_with("RawString")]
struct Bar {
	_max: u32,
	#[skip_name_expansion]
	#[replace_typename_with(SomeOtherName)]
	_moritz: u32,
}

#[derive(GenericTypeInfo)]
#[expand_name_with("Enum")]
enum Buzz {
	_Nothing,
	_NamedStruct { xyz: char },
	_UnnamedStruct(char, u16),
}

#[test]
fn test_name_expansion() {
	let foo_path = <Foo<MyStruct> as TypeInfo>::type_info().path;
	let TypeDef::Composite(foo_def) = <Foo<MyStruct> as TypeInfo>::type_info().type_def.into()
	else {
		panic!()
	};

	// The typename is constructed from the name of the struct in the source code
	// + "For" + the provided expansion (in this case resolved through a generic)
	assert_eq!(&"FooForExample", foo_path.segments.last().unwrap());

	// Because the type in the code for this field is "T", this will also
	// be used for the typename
	assert_eq!("TForExample", foo_def.fields[0].type_name.unwrap());

	// Using the "skip_name_expansion" attribute will prevent the expansion
	// from being applied to the respective typename
	assert_eq!("u16", foo_def.fields[1].type_name.unwrap());

	// Using the "replace_typename_with" attribute will allow you to replace
	// the provided identifier instead of the type name from the code
	assert_eq!("MuchCoolerTypenameForExample", foo_def.fields[2].type_name.unwrap());

	let bar_path = <Bar as TypeInfo>::type_info().path;
	let TypeDef::Composite(bar_def) = <Bar as TypeInfo>::type_info().type_def.into() else {
		panic!()
	};

	// You can also pass raw strings to "expand_name_with"
	assert_eq!(&"BarForRawString", bar_path.segments.last().unwrap());
	assert_eq!("u32ForRawString", bar_def.fields[0].type_name.unwrap());

	// You can apply both "replace_typename_with" and "skip_name_expansion"
	// to the same item
	assert_eq!("SomeOtherName", bar_def.fields[1].type_name.unwrap());

	let buzz_path = <Buzz as TypeInfo>::type_info().path;
	let TypeDef::Variant(buzz_def) = <Buzz as TypeInfo>::type_info().type_def.into() else {
		panic!()
	};

	// The GenericTypeInfo macro can also be applied to Enums
	assert_eq!(&"BuzzForEnum", buzz_path.segments.last().unwrap());

	// The individual variant names will NOT be expanded!
	assert_eq!("_Nothing", buzz_def.variants[0].name);

	// If the variant is a named struct, the field names will be expanded, though!
	assert_eq!("_NamedStruct", buzz_def.variants[1].name);
	assert_eq!("charForEnum", buzz_def.variants[1].fields[0].type_name.unwrap());

	// Also works for unnamed structs
	assert_eq!("_UnnamedStruct", buzz_def.variants[2].name);
	assert_eq!("charForEnum", buzz_def.variants[2].fields[0].type_name.unwrap());
	assert_eq!("u16ForEnum", buzz_def.variants[2].fields[1].type_name.unwrap());
}
