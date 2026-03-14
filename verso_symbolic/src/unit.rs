use crate::dim::{BaseDim, Dimension};
use std::fmt;

/// SI prefix with its scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SiPrefix {
    Pico,  // p, 10^-12
    Nano,  // n, 10^-9
    Micro, // μ, 10^-6
    Milli, // m, 10^-3
    Centi, // c, 10^-2
    Kilo,  // k, 10^3
    Mega,  // M, 10^6
    Giga,  // G, 10^9
    Tera,  // T, 10^12
}

impl SiPrefix {
    pub fn factor(&self) -> f64 {
        match self {
            SiPrefix::Pico => 1e-12,
            SiPrefix::Nano => 1e-9,
            SiPrefix::Micro => 1e-6,
            SiPrefix::Milli => 1e-3,
            SiPrefix::Centi => 1e-2,
            SiPrefix::Kilo => 1e3,
            SiPrefix::Mega => 1e6,
            SiPrefix::Giga => 1e9,
            SiPrefix::Tera => 1e12,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            SiPrefix::Pico => "p",
            SiPrefix::Nano => "n",
            SiPrefix::Micro => "μ",
            SiPrefix::Milli => "m",
            SiPrefix::Centi => "c",
            SiPrefix::Kilo => "k",
            SiPrefix::Mega => "M",
            SiPrefix::Giga => "G",
            SiPrefix::Tera => "T",
        }
    }

    /// All prefixes sorted by factor (largest first) for display selection.
    pub fn all_descending() -> &'static [SiPrefix] {
        &[
            SiPrefix::Tera,
            SiPrefix::Giga,
            SiPrefix::Mega,
            SiPrefix::Kilo,
            SiPrefix::Centi,
            SiPrefix::Milli,
            SiPrefix::Micro,
            SiPrefix::Nano,
            SiPrefix::Pico,
        ]
    }

    /// Try to parse a prefix from the start of a string.
    /// Returns the prefix and the remaining string.
    fn from_start(s: &str) -> Option<(SiPrefix, &str)> {
        // Try multi-char prefixes first, then single-char
        // "mu" and "μ" for micro
        if let Some(rest) = s.strip_prefix("mu") {
            return Some((SiPrefix::Micro, rest));
        }
        if let Some(rest) = s.strip_prefix('μ') {
            return Some((SiPrefix::Micro, rest));
        }
        let first = s.chars().next()?;
        let rest = &s[first.len_utf8()..];
        let prefix = match first {
            'p' => SiPrefix::Pico,
            'n' => SiPrefix::Nano,
            'm' => SiPrefix::Milli,
            'c' => SiPrefix::Centi,
            'k' => SiPrefix::Kilo,
            'M' => SiPrefix::Mega,
            'G' => SiPrefix::Giga,
            'T' => SiPrefix::Tera,
            _ => return None,
        };
        Some((prefix, rest))
    }
}

/// SI base units. Note: gram (not kilogram) is the parseable base for mass,
/// with an implicit scale of 0.001 to the SI base unit kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BaseUnit {
    Meter,   // m → L
    Gram,    // g → M (scale 0.001 relative to kg)
    Second,  // s → T
    Kelvin,  // K → Theta
    Ampere,  // A → I
    Mole,    // mol → N
    Candela, // cd → J
}

impl BaseUnit {
    pub fn symbol(&self) -> &'static str {
        match self {
            BaseUnit::Meter => "m",
            BaseUnit::Gram => "g",
            BaseUnit::Second => "s",
            BaseUnit::Kelvin => "K",
            BaseUnit::Ampere => "A",
            BaseUnit::Mole => "mol",
            BaseUnit::Candela => "cd",
        }
    }

    pub fn dimension(&self) -> BaseDim {
        match self {
            BaseUnit::Meter => BaseDim::L,
            BaseUnit::Gram => BaseDim::M,
            BaseUnit::Second => BaseDim::T,
            BaseUnit::Kelvin => BaseDim::Theta,
            BaseUnit::Ampere => BaseDim::I,
            BaseUnit::Mole => BaseDim::N,
            BaseUnit::Candela => BaseDim::J,
        }
    }

    /// Scale factor to convert to the SI base unit.
    /// All are 1.0 except gram (0.001, since kg is the SI base for mass).
    pub fn si_scale(&self) -> f64 {
        match self {
            BaseUnit::Gram => 0.001,
            _ => 1.0,
        }
    }

    fn from_symbol(s: &str) -> Option<BaseUnit> {
        match s {
            "m" => Some(BaseUnit::Meter),
            "g" => Some(BaseUnit::Gram),
            "s" => Some(BaseUnit::Second),
            "K" => Some(BaseUnit::Kelvin),
            "A" => Some(BaseUnit::Ampere),
            "mol" => Some(BaseUnit::Mole),
            "cd" => Some(BaseUnit::Candela),
            _ => None,
        }
    }
}

/// Named derived SI units, each a shorthand for base unit combinations.
struct DerivedEntry {
    symbol: &'static str,
    dimension: &'static [(BaseDim, i32)],
    scale: f64,
}

const DERIVED_UNITS: &[DerivedEntry] = &[
    DerivedEntry {
        symbol: "N",
        dimension: &[(BaseDim::M, 1), (BaseDim::L, 1), (BaseDim::T, -2)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "J",
        dimension: &[(BaseDim::M, 1), (BaseDim::L, 2), (BaseDim::T, -2)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "W",
        dimension: &[(BaseDim::M, 1), (BaseDim::L, 2), (BaseDim::T, -3)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "Pa",
        dimension: &[(BaseDim::M, 1), (BaseDim::L, -1), (BaseDim::T, -2)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "Hz",
        dimension: &[(BaseDim::T, -1)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "C",
        dimension: &[(BaseDim::I, 1), (BaseDim::T, 1)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "V",
        dimension: &[(BaseDim::M, 1), (BaseDim::L, 2), (BaseDim::T, -3), (BaseDim::I, -1)],
        scale: 1.0,
    },
    DerivedEntry {
        symbol: "Ohm",
        dimension: &[
            (BaseDim::M, 1),
            (BaseDim::L, 2),
            (BaseDim::T, -3),
            (BaseDim::I, -2),
        ],
        scale: 1.0,
    },
];

fn lookup_derived(symbol: &str) -> Option<(Dimension, f64)> {
    // Also accept Ω as Ohm
    let sym = if symbol == "Ω" { "Ohm" } else { symbol };
    for entry in DERIVED_UNITS {
        if entry.symbol == sym {
            let mut dim = Dimension::dimensionless();
            for &(base, exp) in entry.dimension {
                dim = dim.mul(&Dimension::single(base, exp));
            }
            return Some((dim, entry.scale));
        }
    }
    None
}

/// A physical unit combining a dimension with a scale factor relative to base SI.
#[derive(Debug, Clone)]
pub struct Unit {
    pub dimension: Dimension,
    pub scale: f64,
    pub display: String,
}

impl PartialEq for Unit {
    fn eq(&self, other: &Self) -> bool {
        self.dimension == other.dimension && (self.scale - other.scale).abs() < 1e-15
    }
}

impl Unit {
    /// Create a unit from a base unit with optional prefix.
    pub fn from_base(base: BaseUnit, prefix: Option<SiPrefix>) -> Unit {
        let scale = base.si_scale() * prefix.map_or(1.0, |p| p.factor());
        let display = match prefix {
            Some(p) => format!("{}{}", p.symbol(), base.symbol()),
            None => base.symbol().to_string(),
        };
        Unit {
            dimension: Dimension::single(base.dimension(), 1),
            scale,
            display,
        }
    }

    /// Create a unit from a derived unit with optional prefix.
    pub fn from_derived(symbol: &str, prefix: Option<SiPrefix>) -> Option<Unit> {
        let (dimension, base_scale) = lookup_derived(symbol)?;
        let scale = base_scale * prefix.map_or(1.0, |p| p.factor());
        let display = match prefix {
            Some(p) => format!("{}{}", p.symbol(), symbol),
            None => symbol.to_string(),
        };
        Some(Unit {
            dimension,
            scale,
            display,
        })
    }

    /// Multiply two units.
    pub fn mul(&self, other: &Unit) -> Unit {
        Unit {
            dimension: self.dimension.mul(&other.dimension),
            scale: self.scale * other.scale,
            display: format!("{}*{}", self.display, other.display),
        }
    }

    /// Invert a unit.
    pub fn inv(&self) -> Unit {
        Unit {
            dimension: self.dimension.inv(),
            scale: 1.0 / self.scale,
            display: format!("1/{}", self.display),
        }
    }

    /// Raise a unit to an integer power.
    pub fn pow(&self, n: i32) -> Unit {
        Unit {
            dimension: self.dimension.pow(n),
            scale: self.scale.powi(n),
            display: if n == 1 {
                self.display.clone()
            } else {
                format!("{}^{}", self.display, n)
            },
        }
    }
}

impl Unit {
    /// Given a unit symbol (e.g., "km", "kN"), return the base SI unit string
    /// for the same dimension (e.g., "m", "N").
    pub fn base_si_for_symbol(symbol: &str) -> Option<String> {
        let (dim, _) = lookup_unit(symbol)?;
        Some(base_si_display(&dim))
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display)
    }
}

/// Build the base SI unit string for a dimension.
///
/// Prefers named derived units (N, J, W, Pa, Hz, C, V, Ohm) when the dimension
/// matches exactly. Otherwise constructs from base SI units (m, kg, s, K, A, mol, cd).
pub fn base_si_display(dim: &Dimension) -> String {
    if dim.is_dimensionless() {
        return "1".to_string();
    }

    // Check derived units first
    for entry in DERIVED_UNITS {
        let mut d = Dimension::dimensionless();
        for &(base, exp) in entry.dimension {
            d = d.mul(&Dimension::single(base, exp));
        }
        if d == *dim {
            return entry.symbol.to_string();
        }
    }

    // Build from base SI units
    let base_sym = |b: &BaseDim| -> &str {
        match b {
            BaseDim::L => "m",
            BaseDim::M => "kg",
            BaseDim::T => "s",
            BaseDim::Theta => "K",
            BaseDim::I => "A",
            BaseDim::N => "mol",
            BaseDim::J => "cd",
        }
    };

    let mut num = Vec::new();
    let mut den = Vec::new();
    for (&b, &e) in dim.exponents() {
        let sym = base_sym(&b);
        if e > 0 {
            if e == 1 {
                num.push(sym.to_string());
            } else {
                num.push(format!("{}^{}", sym, e));
            }
        } else {
            let abs = -e;
            if abs == 1 {
                den.push(sym.to_string());
            } else {
                den.push(format!("{}^{}", sym, abs));
            }
        }
    }

    if num.is_empty() {
        format!("1/{}", den.join("*"))
    } else if den.is_empty() {
        num.join("*")
    } else {
        format!("{}/{}", num.join("*"), den.join("*"))
    }
}

/// Look up a unit symbol string, returning its dimension and scale.
/// Tries: exact derived unit, exact base unit, prefix+derived, prefix+base.
pub fn lookup_unit(symbol: &str) -> Option<(Dimension, f64)> {
    // 1. Exact derived unit match (e.g., "N", "Hz", "Pa")
    if let Some(result) = lookup_derived(symbol) {
        return Some(result);
    }

    // 2. Exact base unit match (e.g., "m", "s", "kg", "mol")
    // Special case: "kg" = kilo + gram
    if symbol == "kg" {
        return Some((Dimension::single(BaseDim::M, 1), 1.0));
    }
    if let Some(base) = BaseUnit::from_symbol(symbol) {
        return Some((Dimension::single(base.dimension(), 1), base.si_scale()));
    }

    // 3. Try prefix + remainder
    if let Some((prefix, rest)) = SiPrefix::from_start(symbol) {
        // Prefix + derived unit (e.g., "kN", "MHz")
        if let Some((dim, base_scale)) = lookup_derived(rest) {
            return Some((dim, base_scale * prefix.factor()));
        }
        // Prefix + base unit (e.g., "km", "ns", "mg")
        if let Some(base) = BaseUnit::from_symbol(rest) {
            return Some((
                Dimension::single(base.dimension(), 1),
                base.si_scale() * prefix.factor(),
            ));
        }
    }

    None
}

/// Choose the best SI prefix for a value in base SI units.
/// Returns (scaled_value, display_string).
/// E.g., best_prefix(0.001, "m") → (1.0, "mm")
///       best_prefix(3000.0, "m") → (3.0, "km")
pub fn best_prefix(value: f64, base_symbol: &str) -> (f64, String) {
    if value == 0.0 {
        return (0.0, base_symbol.to_string());
    }

    let abs_val = value.abs();

    for prefix in SiPrefix::all_descending() {
        let scaled = abs_val / prefix.factor();
        if scaled >= 1.0 && scaled < 1000.0 {
            return (
                value / prefix.factor(),
                format!("{}{}", prefix.symbol(), base_symbol),
            );
        }
    }

    // No prefix fits well, use base unit
    (value, base_symbol.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_base_units() {
        let (dim, scale) = lookup_unit("m").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::L, 1));
        assert_eq!(scale, 1.0);

        let (dim, scale) = lookup_unit("s").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::T, 1));
        assert_eq!(scale, 1.0);

        let (dim, scale) = lookup_unit("kg").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::M, 1));
        assert_eq!(scale, 1.0);
    }

    #[test]
    fn lookup_gram() {
        let (dim, scale) = lookup_unit("g").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::M, 1));
        assert_eq!(scale, 0.001);
    }

    #[test]
    fn lookup_prefixed_base() {
        let (dim, scale) = lookup_unit("km").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::L, 1));
        assert_eq!(scale, 1e3);

        let (dim, scale) = lookup_unit("ns").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::T, 1));
        assert_eq!(scale, 1e-9);

        let (dim, scale) = lookup_unit("mg").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::M, 1));
        assert!((scale - 1e-6).abs() < 1e-20);
    }

    #[test]
    fn lookup_derived_units() {
        let (dim, scale) = lookup_unit("N").unwrap();
        assert_eq!(dim, Dimension::parse("[M L T^-2]").unwrap());
        assert_eq!(scale, 1.0);

        let (dim, scale) = lookup_unit("Hz").unwrap();
        assert_eq!(dim, Dimension::parse("[T^-1]").unwrap());
        assert_eq!(scale, 1.0);
    }

    #[test]
    fn lookup_prefixed_derived() {
        let (dim, scale) = lookup_unit("kN").unwrap();
        assert_eq!(dim, Dimension::parse("[M L T^-2]").unwrap());
        assert_eq!(scale, 1e3);

        let (dim, scale) = lookup_unit("MHz").unwrap();
        assert_eq!(dim, Dimension::parse("[T^-1]").unwrap());
        assert_eq!(scale, 1e6);
    }

    #[test]
    fn unit_arithmetic() {
        let meter = Unit::from_base(BaseUnit::Meter, None);
        let second = Unit::from_base(BaseUnit::Second, None);
        let velocity = meter.mul(&second.inv());
        assert_eq!(velocity.dimension, Dimension::parse("[L T^-1]").unwrap());
        assert_eq!(velocity.scale, 1.0);
    }

    #[test]
    fn unit_pow() {
        let meter = Unit::from_base(BaseUnit::Meter, None);
        let area = meter.pow(2);
        assert_eq!(area.dimension, Dimension::parse("[L^2]").unwrap());
        assert_eq!(area.scale, 1.0);
    }

    #[test]
    fn best_prefix_selection() {
        let (val, unit) = best_prefix(0.001, "m");
        assert_eq!(unit, "mm");
        assert!((val - 1.0).abs() < 1e-10);

        let (val, unit) = best_prefix(3000.0, "m");
        assert_eq!(unit, "km");
        assert!((val - 3.0).abs() < 1e-10);

        let (val, unit) = best_prefix(1.5e6, "Hz");
        assert_eq!(unit, "MHz");
        assert!((val - 1.5).abs() < 1e-10);
    }

    #[test]
    fn base_si_for_prefixed_base() {
        assert_eq!(Unit::base_si_for_symbol("km").unwrap(), "m");
        assert_eq!(Unit::base_si_for_symbol("mg").unwrap(), "kg");
        assert_eq!(Unit::base_si_for_symbol("ns").unwrap(), "s");
    }

    #[test]
    fn base_si_for_derived() {
        assert_eq!(Unit::base_si_for_symbol("N").unwrap(), "N");
        assert_eq!(Unit::base_si_for_symbol("kN").unwrap(), "N");
        assert_eq!(Unit::base_si_for_symbol("MHz").unwrap(), "Hz");
    }

    #[test]
    fn base_si_for_base_unit() {
        assert_eq!(Unit::base_si_for_symbol("m").unwrap(), "m");
        assert_eq!(Unit::base_si_for_symbol("s").unwrap(), "s");
        assert_eq!(Unit::base_si_for_symbol("kg").unwrap(), "kg");
    }

    #[test]
    fn base_si_unknown_returns_none() {
        assert!(Unit::base_si_for_symbol("xyz").is_none());
    }

    #[test]
    fn unknown_symbol_returns_none() {
        assert!(lookup_unit("xyz").is_none());
        assert!(lookup_unit("foo").is_none());
    }

    #[test]
    fn kg_special_case() {
        // kg is the SI base for mass — scale should be 1.0
        let (dim, scale) = lookup_unit("kg").unwrap();
        assert_eq!(dim, Dimension::single(BaseDim::M, 1));
        assert_eq!(scale, 1.0);

        // g has scale 0.001
        let (_, g_scale) = lookup_unit("g").unwrap();
        assert_eq!(g_scale, 0.001);

        // mg has scale 10^-6
        let (_, mg_scale) = lookup_unit("mg").unwrap();
        assert!((mg_scale - 1e-6).abs() < 1e-20);
    }
}
