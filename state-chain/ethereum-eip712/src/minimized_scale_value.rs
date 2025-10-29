use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MinimizedScaleValue {
	NamedStruct(Vec<(String, MinimizedScaleValue)>),
	Sequence(Vec<MinimizedScaleValue>),
	Primitive(scale_value::Primitive),
}

impl TryFrom<Value> for MinimizedScaleValue {
	type Error = &'static str;

	fn try_from(value: Value) -> Result<Self, Self::Error> {
		match value.value {
			ValueDef::Composite(Composite::Named(fs)) => Ok(Self::NamedStruct(
				fs.into_iter()
					.map(|(name, v)| Ok((name, MinimizedScaleValue::try_from(v)?)))
					.collect::<Result<Vec<(String, Self)>, &'static str>>()?,
			)),
			ValueDef::Composite(Composite::Unnamed(fs)) => Ok(Self::Sequence(
				fs.into_iter()
					.map(MinimizedScaleValue::try_from)
					.collect::<Result<Vec<Self>, &'static str>>()?,
			)),
			ValueDef::Variant(_) =>
				Err("scale value with variant cannot be converted to MinimizedScaleValue"),
			ValueDef::Primitive(p) => Ok(Self::Primitive(p)),
			ValueDef::BitSequence(_) => Err("BitSequence not supported"),
		}
	}
}

impl MinimizedScaleValue {
	pub fn get_struct_field(&self, field_name: String) -> Result<Self, String> {
		match &self {
			Self::NamedStruct(fs) => fs
				.iter()
				.find(|(name, _)| *name == field_name)
				.ok_or(format!("field with this name not found: {:?}", field_name))
				.map(|(_, v)| (*v).clone()),
			_ => Err("this value is not a struct".to_string()),
		}
	}

	// of the kind U256
	#[allow(clippy::result_unit_err)]
	pub fn extract_primitive_types<T: TryFrom<u128>>(&self) -> Result<Vec<T>, ()> {
		if let Self::NamedStruct(fs) = self.clone() {
			if fs.len() != 1 {
				return Err(());
			}
			fs[0].1.extract_primitive_array::<T>().map_err(|_| ())
		} else {
			Err(())
		}
	}

	// of the kind [_;N]
	#[allow(clippy::result_unit_err)]
	pub fn extract_primitive_array<T: TryFrom<u128>>(&self) -> Result<Vec<T>, ()> {
		if let Self::Sequence(fs) = self.clone() {
			fs.into_iter()
				.map(|v| {
					if let Self::Primitive(Primitive::U128(el)) = v {
						Ok(T::try_from(el).map_err(|_| ())?)
					} else {
						Err(())
					}
				})
				.collect::<Result<Vec<T>, _>>()
		} else {
			Err(())
		}
	}

	#[allow(clippy::result_unit_err)]
	pub fn extract_hex_bytes(&self) -> Result<Vec<u8>, ()> {
		if let Self::Primitive(Primitive::String(s)) = self.clone() {
			hex::decode(s).map_err(|_| ())
		} else {
			Err(())
		}
	}
}

impl From<MinimizedScaleValue> for Value {
	fn from(value: MinimizedScaleValue) -> Self {
		match value {
			MinimizedScaleValue::NamedStruct(fs) => Value {
				value: ValueDef::Composite(Composite::Named(
					fs.into_iter().map(|(name, v)| (name, Value::from(v))).collect(),
				)),
				context: (),
			},
			MinimizedScaleValue::Sequence(fs) => Value {
				value: ValueDef::Composite(Composite::Unnamed(
					fs.into_iter().map(Value::from).collect(),
				)),
				context: (),
			},
			MinimizedScaleValue::Primitive(p) =>
				Value { value: ValueDef::Primitive(p), context: () },
		}
	}
}
