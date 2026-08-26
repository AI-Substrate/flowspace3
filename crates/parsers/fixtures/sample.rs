//! Fixture for the parser exemplar test. Every construct here is load-bearing.

pub mod geometry {
    /// A rectangle.
    pub struct Rect {
        pub width: f64,
        pub height: f64,
    }

    impl Rect {
        pub fn new(width: f64, height: f64) -> Self {
            // `Self { .. }` parses as `struct_expression`, which the naive
            // substring classifier promotes to a phantom type element.
            Self { width, height }
        }

        pub fn area(&self) -> f64 {
            self.width * self.height
        }
    }

    pub trait Shape {
        fn area(&self) -> f64;
    }

    pub enum Kind {
        Square,
        Oblong,
    }
}

pub const MAX_SIDES: u32 = 4;

pub fn main_entry() {
    // A function declared inside a function is still a declaration, and it
    // belongs to the function it is written in - not to the file.
    fn half(value: f64) -> f64 {
        value / 2.0
    }

    let _ = geometry::Rect::new(half(2.0), 2.0);
}
