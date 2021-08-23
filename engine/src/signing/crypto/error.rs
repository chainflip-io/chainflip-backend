use std::fmt;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct InvalidKey(pub Vec<usize> /* blamed parties */);

impl fmt::Display for InvalidKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self)
    }
}

impl std::error::Error for InvalidKey {}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct InvalidSS(pub Vec<usize> /* blamed parties */);

impl fmt::Display for InvalidSS {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self)
    }
}

impl std::error::Error for InvalidSS {}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct InvalidSig;

impl fmt::Display for InvalidSig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", &self)
    }
}

impl std::error::Error for InvalidSig {}
