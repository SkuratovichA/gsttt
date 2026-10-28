mod strategies;

mod shapes {
    pub mod round {
        pub struct Ball;
    }

    use round::Ball;

    pub struct Circle;

    pub fn ball() -> Ball {
        Ball
    }
}

pub use shapes::Circle;
pub use shapes::round::Ball;

pub fn circle() -> shapes::Circle {
    shapes::Circle
}
