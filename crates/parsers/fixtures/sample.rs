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
    let _ = geometry::Rect::new(1.0, 2.0);
}
