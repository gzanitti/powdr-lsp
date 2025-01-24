use std::ops::Range;

pub type Span = Range<usize>;

pub trait HasSpan {
    fn span(&self) -> Span;
}

impl HasSpan for Span {
    fn span(&self) -> Span {
        self.clone()
    }
}
