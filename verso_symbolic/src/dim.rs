use std::collections::BTreeMap;
use std::fmt;

/// Base physical dimensions (SI).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseDim {
    L,     // Length
    M,     // Mass
    T,     // Time
    Theta, // Temperature
    I,     // Electric current
    N,     // Amount of substance
    J,     // Luminous intensity
}

impl BaseDim {
    pub fn from_str(s: &str) -> Option<BaseDim> {
        match s {
            "L" => Some(BaseDim::L),
            "M" => Some(BaseDim::M),
            "T" => Some(BaseDim::T),
            "Θ" | "Theta" => Some(BaseDim::Theta),
            "I" => Some(BaseDim::I),
            "N" => Some(BaseDim::N),
            "J" => Some(BaseDim::J),
            _ => None,
        }
    }
}

impl fmt::Display for BaseDim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaseDim::L => write!(f, "L"),
            BaseDim::M => write!(f, "M"),
            BaseDim::T => write!(f, "T"),
            BaseDim::Theta => write!(f, "Θ"),
            BaseDim::I => write!(f, "I"),
            BaseDim::N => write!(f, "N"),
            BaseDim::J => write!(f, "J"),
        }
    }
}

/// A physical dimension as a product of base dimensions with integer exponents.
/// E.g., force = M L T^-2 is represented as {M: 1, L: 1, T: -2}.
/// Dimensionless = empty map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    exponents: BTreeMap<BaseDim, i32>,
}

impl Dimension {
    pub fn dimensionless() -> Self {
        Dimension {
            exponents: BTreeMap::new(),
        }
    }

    pub fn is_dimensionless(&self) -> bool {
        self.exponents.values().all(|&e| e == 0)
    }

    /// Create a dimension with a single base dimension and exponent.
    pub fn single(base: BaseDim, exp: i32) -> Self {
        let mut exponents = BTreeMap::new();
        if exp != 0 {
            exponents.insert(base, exp);
        }
        Dimension { exponents }
    }

    pub fn exponents(&self) -> &BTreeMap<BaseDim, i32> {
        &self.exponents
    }

    /// Multiply dimensions (add exponents).
    pub fn mul(&self, other: &Dimension) -> Dimension {
        let mut result = self.exponents.clone();
        for (&base, &exp) in &other.exponents {
            *result.entry(base).or_insert(0) += exp;
        }
        result.retain(|_, e| *e != 0);
        Dimension { exponents: result }
    }

    /// Inverse dimension (negate exponents).
    pub fn inv(&self) -> Dimension {
        let exponents = self.exponents.iter().map(|(&b, &e)| (b, -e)).collect();
        Dimension { exponents }
    }

    /// Raise to integer power (multiply all exponents).
    pub fn pow(&self, n: i32) -> Dimension {
        if n == 0 {
            return Dimension::dimensionless();
        }
        let exponents = self
            .exponents
            .iter()
            .map(|(&b, &e)| (b, e * n))
            .filter(|(_, e)| *e != 0)
            .collect();
        Dimension { exponents }
    }

    /// Parse a dimension from bracket notation: `[M L T^-2]`
    pub fn parse(s: &str) -> Result<Dimension, String> {
        let s = s.trim();
        let s = s
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| "dimension must be enclosed in brackets, e.g. [M L T^-2]".to_string())?;
        let s = s.trim();

        if s == "1" || s.is_empty() {
            return Ok(Dimension::dimensionless());
        }

        let mut exponents = BTreeMap::new();
        for token in s.split_whitespace() {
            let (base_str, exp) = if let Some(caret_pos) = token.find('^') {
                let base_str = &token[..caret_pos];
                let exp_str = &token[caret_pos + 1..];
                let exp: i32 = exp_str
                    .parse()
                    .map_err(|_| format!("invalid exponent '{}' in dimension", exp_str))?;
                (base_str, exp)
            } else {
                (token, 1)
            };

            let base = BaseDim::from_str(base_str)
                .ok_or_else(|| format!("unknown base dimension '{}'", base_str))?;

            *exponents.entry(base).or_insert(0) += exp;
        }

        exponents.retain(|_, e| *e != 0);
        Ok(Dimension { exponents })
    }
}

impl fmt::Display for Dimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_dimensionless() {
            return write!(f, "[1]");
        }
        write!(f, "[")?;
        let mut first = true;
        for (base, exp) in &self.exponents {
            if !first {
                write!(f, " ")?;
            }
            first = false;
            if *exp == 1 {
                write!(f, "{}", base)?;
            } else {
                write!(f, "{}^{}", base, exp)?;
            }
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dim(s: &str) -> Dimension {
        Dimension::parse(s).unwrap()
    }

    #[test]
    fn parse_simple_dimension() {
        let d = dim("[L]");
        assert_eq!(d.to_string(), "[L]");
    }

    #[test]
    fn parse_compound_dimension() {
        let d = dim("[M L T^-2]");
        assert_eq!(d.to_string(), "[L M T^-2]"); // BTreeMap sorts by key
    }

    #[test]
    fn parse_dimensionless() {
        assert!(dim("[1]").is_dimensionless());
    }

    #[test]
    fn dimension_mul() {
        let mass = dim("[M]");
        let accel = dim("[L T^-2]");
        let force = mass.mul(&accel);
        assert_eq!(force, dim("[M L T^-2]"));
    }

    #[test]
    fn dimension_inv() {
        let time = dim("[T]");
        let freq = time.inv();
        assert_eq!(freq, dim("[T^-1]"));
    }

    #[test]
    fn dimension_pow() {
        let length = dim("[L]");
        let area = length.pow(2);
        assert_eq!(area, dim("[L^2]"));
    }

    #[test]
    fn dimension_pow_zero() {
        let length = dim("[L]");
        assert!(length.pow(0).is_dimensionless());
    }

    #[test]
    fn dimension_pow_negative() {
        let length = dim("[L]");
        assert_eq!(length.pow(-2), dim("[L^-2]"));
    }

    #[test]
    fn single_zero_exponent_is_dimensionless() {
        let d = Dimension::single(BaseDim::L, 0);
        assert!(d.is_dimensionless());
    }

    #[test]
    fn parse_missing_brackets() {
        assert!(Dimension::parse("M L T").is_err());
    }

    #[test]
    fn parse_invalid_exponent() {
        assert!(Dimension::parse("[L^abc]").is_err());
    }

    #[test]
    fn parse_unknown_base_dim() {
        assert!(Dimension::parse("[X]").is_err());
    }

    #[test]
    fn parse_empty_brackets() {
        assert!(Dimension::parse("[]").unwrap().is_dimensionless());
    }

    #[test]
    fn base_dim_from_str_unknown_returns_none() {
        assert_eq!(BaseDim::from_str("X"), None);
        assert_eq!(BaseDim::from_str(""), None);
    }
}
