use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// Exact rational number with i64 numerator and denominator.
/// Invariants: den > 0, gcd(|num|, den) == 1, zero is 0/1.
#[derive(Clone, Copy, Debug, Eq, Hash)]
pub struct Rational {
    num: i64,
    den: i64,
}

impl Rational {
    pub const ZERO: Rational = Rational { num: 0, den: 1 };
    pub const ONE: Rational = Rational { num: 1, den: 1 };
    pub const NEG_ONE: Rational = Rational { num: -1, den: 1 };
    pub const TWO: Rational = Rational { num: 2, den: 1 };

    pub fn new(num: i64, den: i64) -> Rational {
        assert!(den != 0, "Rational: zero denominator");
        if num == 0 {
            return Rational::ZERO;
        }
        let sign = if den < 0 { -1 } else { 1 };
        let g = gcd(num.abs(), den.abs());
        Rational {
            num: sign * num / g,
            den: sign * den / g,
        }
    }

    pub fn from_i64(n: i64) -> Rational {
        Rational { num: n, den: 1 }
    }

    pub fn num(&self) -> i64 {
        self.num
    }

    pub fn den(&self) -> i64 {
        self.den
    }

    pub fn value(&self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn is_integer(&self) -> bool {
        self.den == 1
    }

    pub fn is_even(&self) -> bool {
        self.is_integer() && self.num % 2 == 0
    }

    pub fn is_odd(&self) -> bool {
        self.is_integer() && self.num % 2 != 0
    }

    pub fn is_zero(&self) -> bool {
        self.num == 0
    }

    pub fn is_positive(&self) -> bool {
        self.num > 0
    }

    pub fn is_negative(&self) -> bool {
        self.num < 0
    }

    pub fn abs(&self) -> Rational {
        Rational {
            num: self.num.abs(),
            den: self.den,
        }
    }

    /// Integer floor: largest integer <= self.
    pub fn floor(&self) -> i64 {
        if self.num >= 0 || self.num % self.den == 0 {
            self.num / self.den
        } else {
            self.num / self.den - 1
        }
    }

    /// Fractional part: self - floor(self). Always in [0, 1).
    pub fn fract(&self) -> Rational {
        *self - Rational::from_i64(self.floor())
    }

    /// Euclidean remainder: result in [0, modulus).
    pub fn rem_euclid(&self, modulus: Rational) -> Rational {
        assert!(
            modulus.is_positive(),
            "rem_euclid: modulus must be positive"
        );
        let q = (*self / modulus).floor();
        *self - Rational::from_i64(q) * modulus
    }
}

impl PartialEq for Rational {
    fn eq(&self, other: &Self) -> bool {
        // Both are always GCD-reduced with positive denominator
        self.num == other.num && self.den == other.den
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // a/b vs c/d => a*d vs c*b (both denominators positive)
        (self.num * other.den).cmp(&(other.num * self.den))
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.den == 1 {
            write!(f, "{}", self.num)
        } else {
            write!(f, "{}/{}", self.num, self.den)
        }
    }
}

impl From<i64> for Rational {
    fn from(n: i64) -> Self {
        Rational::from_i64(n)
    }
}

impl Add for Rational {
    type Output = Rational;
    fn add(self, rhs: Rational) -> Rational {
        Rational::new(self.num * rhs.den + rhs.num * self.den, self.den * rhs.den)
    }
}

impl Sub for Rational {
    type Output = Rational;
    fn sub(self, rhs: Rational) -> Rational {
        Rational::new(self.num * rhs.den - rhs.num * self.den, self.den * rhs.den)
    }
}

impl Mul for Rational {
    type Output = Rational;
    fn mul(self, rhs: Rational) -> Rational {
        Rational::new(self.num * rhs.num, self.den * rhs.den)
    }
}

impl Div for Rational {
    type Output = Rational;
    fn div(self, rhs: Rational) -> Rational {
        assert!(!rhs.is_zero(), "Rational: division by zero");
        Rational::new(self.num * rhs.den, self.den * rhs.num)
    }
}

impl Neg for Rational {
    type Output = Rational;
    fn neg(self) -> Rational {
        Rational {
            num: -self.num,
            den: self.den,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_reduces() {
        assert_eq!(Rational::new(4, 6), Rational::new(2, 3));
        assert_eq!(Rational::new(10, 5), Rational::from_i64(2));
    }

    #[test]
    fn new_normalizes_sign() {
        let r = Rational::new(3, -4);
        assert_eq!(r.num(), -3);
        assert_eq!(r.den(), 4);

        let r = Rational::new(-3, -4);
        assert_eq!(r.num(), 3);
        assert_eq!(r.den(), 4);
    }

    #[test]
    fn zero_canonical() {
        assert_eq!(Rational::new(0, 5), Rational::ZERO);
        assert_eq!(Rational::new(0, -3), Rational::ZERO);
    }

    #[test]
    #[should_panic(expected = "zero denominator")]
    fn zero_denominator_panics() {
        Rational::new(1, 0);
    }

    #[test]
    fn value_conversion() {
        assert_eq!(Rational::new(1, 3).value(), 1.0 / 3.0);
        assert_eq!(Rational::from_i64(5).value(), 5.0);
    }

    #[test]
    fn predicates() {
        assert!(Rational::from_i64(4).is_integer());
        assert!(!Rational::new(1, 3).is_integer());
        assert!(Rational::from_i64(4).is_even());
        assert!(!Rational::from_i64(3).is_even());
        assert!(Rational::from_i64(3).is_odd());
        assert!(!Rational::from_i64(4).is_odd());
        assert!(Rational::ZERO.is_zero());
        assert!(Rational::from_i64(1).is_positive());
        assert!(Rational::from_i64(-1).is_negative());
        // Non-integers are neither even nor odd
        assert!(!Rational::new(1, 2).is_even());
        assert!(!Rational::new(1, 2).is_odd());
    }

    #[test]
    fn abs_value() {
        assert_eq!(Rational::new(-3, 4).abs(), Rational::new(3, 4));
        assert_eq!(Rational::new(3, 4).abs(), Rational::new(3, 4));
    }

    #[test]
    fn floor_positive() {
        assert_eq!(Rational::new(7, 3).floor(), 2);
        assert_eq!(Rational::from_i64(5).floor(), 5);
        assert_eq!(Rational::new(1, 3).floor(), 0);
    }

    #[test]
    fn floor_negative() {
        assert_eq!(Rational::new(-7, 3).floor(), -3);
        assert_eq!(Rational::new(-1, 3).floor(), -1);
        assert_eq!(Rational::from_i64(-5).floor(), -5);
        assert_eq!(Rational::new(-6, 3).floor(), -2);
    }

    #[test]
    fn fract_value() {
        assert_eq!(Rational::new(7, 3).fract(), Rational::new(1, 3));
        assert_eq!(Rational::from_i64(5).fract(), Rational::ZERO);
        assert_eq!(Rational::new(-7, 3).fract(), Rational::new(2, 3));
    }

    #[test]
    fn rem_euclid_value() {
        // 9/4 mod 2 = 1/4
        assert_eq!(
            Rational::new(9, 4).rem_euclid(Rational::TWO),
            Rational::new(1, 4)
        );
        // 7/3 mod 2 = 1/3
        assert_eq!(
            Rational::new(7, 3).rem_euclid(Rational::TWO),
            Rational::new(1, 3)
        );
        // 100 mod 2 = 0
        assert_eq!(
            Rational::from_i64(100).rem_euclid(Rational::TWO),
            Rational::ZERO
        );
        // -1/4 mod 2 = 7/4
        assert_eq!(
            Rational::new(-1, 4).rem_euclid(Rational::TWO),
            Rational::new(7, 4)
        );
    }

    #[test]
    fn arithmetic_add() {
        assert_eq!(
            Rational::new(1, 3) + Rational::new(1, 6),
            Rational::new(1, 2)
        );
        assert_eq!(
            Rational::from_i64(2) + Rational::new(1, 3),
            Rational::new(7, 3)
        );
    }

    #[test]
    fn arithmetic_sub() {
        assert_eq!(
            Rational::new(1, 2) - Rational::new(1, 3),
            Rational::new(1, 6)
        );
    }

    #[test]
    fn arithmetic_mul() {
        assert_eq!(
            Rational::new(2, 3) * Rational::new(3, 4),
            Rational::new(1, 2)
        );
    }

    #[test]
    fn arithmetic_div() {
        assert_eq!(
            Rational::new(2, 3) / Rational::new(4, 5),
            Rational::new(5, 6)
        );
    }

    #[test]
    fn arithmetic_neg() {
        assert_eq!(-Rational::new(3, 4), Rational::new(-3, 4));
        assert_eq!(-Rational::ZERO, Rational::ZERO);
    }

    #[test]
    fn display_integer() {
        assert_eq!(format!("{}", Rational::from_i64(42)), "42");
        assert_eq!(format!("{}", Rational::from_i64(-3)), "-3");
        assert_eq!(format!("{}", Rational::ZERO), "0");
    }

    #[test]
    fn display_fraction() {
        assert_eq!(format!("{}", Rational::new(1, 3)), "1/3");
        assert_eq!(format!("{}", Rational::new(-7, 4)), "-7/4");
    }

    #[test]
    fn ordering() {
        assert!(Rational::new(1, 3) < Rational::new(1, 2));
        assert!(Rational::from_i64(-1) < Rational::ZERO);
        assert!(Rational::new(2, 3) > Rational::new(1, 2));
    }

    #[test]
    fn from_i64_trait() {
        let r: Rational = 7.into();
        assert_eq!(r, Rational::from_i64(7));
    }
}
