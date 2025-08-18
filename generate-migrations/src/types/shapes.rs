use crate::types::definition::Shape;


impl<A: Shape> Shape for Vec<A> {
    type Next = Vec<A::Next>;

    fn try_get_next(&self) -> Option<Self::Next> {
        self.iter().map(Shape::try_get_next).collect()
    }
}
