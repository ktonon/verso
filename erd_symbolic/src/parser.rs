use crate::dim::Dimension;
use crate::expr::{
    acos, add, asin, atan, ceil, clamp, constant, cos, cosh, exp, floor, frac_pi, inv, ln, max,
    min, mul, named, neg, pow, quantity, round, scalar, sign, sin, sinh, sqrt, tan, tanh, Expr,
    Index, IndexPosition, NamedConst,
};
use crate::rational::Rational;
use crate::unit::Unit;

// TODO: allow the input of special characters by using a latex style.
// For example \delta becomes δ.
// δ can be interpretted as the Kronecker delta.
// Likewise we can stop parsing pi and require \pi for consistency.
// Likewise \epsilon becomes ε for the Levi-Civita symbol.
//
// The command :lsc short for list special characters will output all supported special characters.
//
// The repl can also support like special character replacement if:
// - the user has typed enough following a \ to disambiguate
// - the user then presses tab

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnexpectedEof,
    UnexpectedToken(String),
    InvalidNumber(String),
    InvalidIndexList,
    InvalidLogBase,
    Expected(String),
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(String),
    Ident(String),
    Pi,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
    Underscore,
    Caret,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Tensor,
    DotOp,
    Colon,
}

pub fn parse_expr(src: &str) -> Result<crate::expr::Expr, ParseError> {
    let tokens = tokenize(src)?;
    let mut parser = Parser::new(tokens);
    let expr = parser.parse_additive()?;
    if !parser.is_eof() {
        return Err(ParseError::UnexpectedToken(parser.peek_string()));
    }
    Ok(expr)
}

fn tokenize(src: &str) -> Result<Vec<Token>, ParseError> {
    let mut chars = src.chars().peekable();
    let mut tokens = Vec::new();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        if ch.is_ascii_digit() || ch == '.' {
            let mut s = String::new();
            let mut dot_count = 0;
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    chars.next();
                } else if c == '.' {
                    dot_count += 1;
                    if dot_count > 1 {
                        break;
                    }
                    s.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(s));
            continue;
        }

        if ch.is_ascii_alphabetic() {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() {
                    s.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Ident(s));
            continue;
        }

        match ch {
            'π' => {
                chars.next();
                tokens.push(Token::Pi);
            }
            '+' => {
                chars.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                chars.next();
                tokens.push(Token::Minus);
            }
            '*' => {
                chars.next();
                tokens.push(Token::Star);
            }
            '/' => {
                chars.next();
                tokens.push(Token::Slash);
            }
            '(' => {
                chars.next();
                tokens.push(Token::LParen);
            }
            ')' => {
                chars.next();
                tokens.push(Token::RParen);
            }
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
            }
            '_' => {
                chars.next();
                tokens.push(Token::Underscore);
            }
            '^' => {
                chars.next();
                tokens.push(Token::Caret);
            }
            '{' => {
                chars.next();
                tokens.push(Token::LBrace);
            }
            '}' => {
                chars.next();
                tokens.push(Token::RBrace);
            }
            '[' => {
                chars.next();
                tokens.push(Token::LBracket);
            }
            ']' => {
                chars.next();
                tokens.push(Token::RBracket);
            }
            '⊗' => {
                chars.next();
                tokens.push(Token::Tensor);
            }
            '⋅' => {
                chars.next();
                tokens.push(Token::DotOp);
            }
            ':' => {
                chars.next();
                tokens.push(Token::Colon);
            }
            _ => {
                return Err(ParseError::UnexpectedToken(ch.to_string()));
            }
        }
    }

    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset)
    }

    fn next(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn peek_string(&self) -> String {
        match self.peek() {
            Some(tok) => format!("{:?}", tok),
            None => "EOF".to_string(),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        let tok = self.next().ok_or(ParseError::UnexpectedEof)?;
        if tok == expected {
            Ok(())
        } else {
            Err(ParseError::Expected(format!("{:?}", expected)))
        }
    }

    fn parse_additive(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.next();
                    let rhs = self.parse_multiplicative()?;
                    expr = add(expr, rhs);
                }
                Some(Token::Minus) => {
                    self.next();
                    let rhs = self.parse_multiplicative()?;
                    expr = add(expr, neg(rhs));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let mut expr = self.parse_unary()?;

        loop {
            match self.peek() {
                Some(Token::Star) | Some(Token::Slash) | Some(Token::Tensor)
                | Some(Token::DotOp) | Some(Token::Colon) => {
                    let op = self.next().unwrap();
                    let rhs = self.parse_unary()?;
                    expr = match op {
                        Token::Slash => mul(expr, inv(rhs)),
                        _ => mul(expr, rhs),
                    };
                }
                _ => {
                    // implicit multiplication (e.g. 2x or 2(x+1))
                    if self.next_starts_primary() {
                        let rhs = self.parse_unary()?;
                        expr = mul(expr, rhs);
                        continue;
                    }
                    break;
                }
            }
        }

        // Check for unit annotation: [unit] on purely numeric expressions
        if matches!(self.peek(), Some(Token::LBracket)) && !expr_has_vars(&expr) {
            self.next(); // consume [
            let unit = self.parse_unit_bracket()?;
            expr = quantity(expr, unit);
        }

        Ok(expr)
    }

    fn parse_power(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        if matches!(self.peek(), Some(Token::Caret))
            && !matches!(self.peek_at(1), Some(Token::LBrace))
        {
            self.next();
            let rhs = self.parse_power()?;
            expr = pow(expr, rhs);
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<crate::expr::Expr, ParseError> {
        match self.peek() {
            Some(Token::Minus) => {
                self.next();
                Ok(neg(self.parse_power()?))
            }
            _ => self.parse_power(),
        }
    }

    fn parse_primary(&mut self) -> Result<crate::expr::Expr, ParseError> {
        match self.next() {
            Some(Token::Number(s)) => {
                if !s.contains('.') {
                    if let Ok(n) = s.parse::<i64>() {
                        return Ok(Expr::Rational(Rational::from_i64(n)));
                    }
                }
                let n: f64 = s
                    .parse()
                    .map_err(|_| ParseError::InvalidNumber(s.clone()))?;
                Ok(constant(n))
            }
            Some(Token::Pi) => Ok(frac_pi(1, 1)),
            Some(Token::Ident(name)) => {
                if name == "pi" {
                    return Ok(frac_pi(1, 1));
                }
                if name == "e" {
                    return Ok(named(NamedConst::E));
                }
                if name == "log" && self.peek() == Some(&Token::Underscore) {
                    return self.parse_log_base_tokens();
                }
                if name.starts_with("log_") {
                    return self.parse_log_base(name);
                }
                if self.peek() == Some(&Token::LParen) && is_known_function(&name) {
                    return self.parse_function_call(name);
                }
                let mut expr = scalar(&name);
                expr = self.parse_indices(expr)?;
                Ok(expr)
            }
            Some(Token::LParen) => {
                let expr = self.parse_additive()?;
                self.expect(Token::RParen)?;
                Ok(expr)
            }
            Some(tok) => Err(ParseError::UnexpectedToken(format!("{:?}", tok))),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn parse_function_call(&mut self, name: String) -> Result<crate::expr::Expr, ParseError> {
        self.expect(Token::LParen)?;
        let mut args = Vec::new();
        if self.peek() != Some(&Token::RParen) {
            loop {
                args.push(self.parse_additive()?);
                match self.peek() {
                    Some(Token::Comma) => {
                        self.next();
                    }
                    _ => break,
                }
            }
        }
        self.expect(Token::RParen)?;

        match name.as_str() {
            "sin" => Ok(sin(expect_arity(args, 1, "sin")?)),
            "cos" => Ok(cos(expect_arity(args, 1, "cos")?)),
            "tan" => Ok(tan(expect_arity(args, 1, "tan")?)),
            "asin" => Ok(asin(expect_arity(args, 1, "asin")?)),
            "acos" => Ok(acos(expect_arity(args, 1, "acos")?)),
            "atan" => Ok(atan(expect_arity(args, 1, "atan")?)),
            "sinh" => Ok(sinh(expect_arity(args, 1, "sinh")?)),
            "cosh" => Ok(cosh(expect_arity(args, 1, "cosh")?)),
            "tanh" => Ok(tanh(expect_arity(args, 1, "tanh")?)),
            "exp" => Ok(exp(expect_arity(args, 1, "exp")?)),
            "ln" => Ok(ln(expect_arity(args, 1, "ln")?)),
            "sign" => Ok(sign(expect_arity(args, 1, "sign")?)),
            "floor" => Ok(floor(expect_arity(args, 1, "floor")?)),
            "ceil" => Ok(ceil(expect_arity(args, 1, "ceil")?)),
            "round" => Ok(round(expect_arity(args, 1, "round")?)),
            "sqrt" => Ok(sqrt(expect_arity(args, 1, "sqrt")?)),
            "min" => {
                let mut args = expect_n(args, 2, "min")?;
                Ok(min(args.remove(0), args.remove(0)))
            }
            "max" => {
                let mut args = expect_n(args, 2, "max")?;
                Ok(max(args.remove(0), args.remove(0)))
            }
            "clamp" => {
                let mut args = expect_n(args, 3, "clamp")?;
                Ok(clamp(args.remove(0), args.remove(0), args.remove(0)))
            }
            _ => Err(ParseError::UnexpectedToken(name)),
        }
    }

    fn parse_log_base(&mut self, name: String) -> Result<crate::expr::Expr, ParseError> {
        let base = if name == "log_" {
            // log_(base)(arg)
            self.expect(Token::LParen)?;
            let base_expr = self.parse_additive()?;
            self.expect(Token::RParen)?;
            base_expr
        } else {
            let suffix = name.trim_start_matches("log_");
            if suffix.is_empty() {
                return Err(ParseError::InvalidLogBase);
            }
            if let Ok(n) = suffix.parse::<f64>() {
                constant(n)
            } else {
                scalar(suffix)
            }
        };

        self.expect(Token::LParen)?;
        let arg = self.parse_additive()?;
        self.expect(Token::RParen)?;

        Ok(mul(ln(arg), inv(ln(base))))
    }

    fn parse_log_base_tokens(&mut self) -> Result<crate::expr::Expr, ParseError> {
        self.expect(Token::Underscore)?;
        let base = if self.peek() == Some(&Token::LParen) {
            self.next();
            let base_expr = self.parse_additive()?;
            self.expect(Token::RParen)?;
            base_expr
        } else {
            match self.next() {
                Some(Token::Number(s)) => constant(
                    s.parse()
                        .map_err(|_| ParseError::InvalidNumber(s.clone()))?,
                ),
                Some(Token::Ident(name)) => scalar(&name),
                _ => return Err(ParseError::InvalidLogBase),
            }
        };
        self.expect(Token::LParen)?;
        let arg = self.parse_additive()?;
        self.expect(Token::RParen)?;
        Ok(mul(ln(arg), inv(ln(base))))
    }

    fn parse_indices(
        &mut self,
        mut expr: crate::expr::Expr,
    ) -> Result<crate::expr::Expr, ParseError> {
        let mut lowers: Vec<Index> = Vec::new();
        let mut uppers: Vec<Index> = Vec::new();

        loop {
            if matches!(self.peek(), Some(Token::Underscore))
                && matches!(self.peek_at(1), Some(Token::LBrace))
            {
                self.next(); // consume _
                self.next(); // consume {
                let indices = self.parse_brace_index_list(IndexPosition::Lower)?;
                lowers.extend(indices);
            } else if matches!(self.peek(), Some(Token::Caret))
                && matches!(self.peek_at(1), Some(Token::LBrace))
            {
                self.next(); // consume ^
                self.next(); // consume {
                let indices = self.parse_brace_index_list(IndexPosition::Upper)?;
                uppers.extend(indices);
            } else {
                break;
            }
        }

        // Parse optional dimension annotation: [M L T^-2]
        let dim = if matches!(self.peek(), Some(Token::LBracket)) {
            self.next(); // consume [
            Some(self.parse_dimension_bracket()?)
        } else {
            None
        };

        if !lowers.is_empty() || !uppers.is_empty() || dim.is_some() {
            let mut all = Vec::new();
            all.extend(lowers);
            all.extend(uppers);
            if let crate::expr::Expr::Var { name, .. } = expr {
                expr = crate::expr::Expr::Var { name, indices: all, dim };
            }
        }

        Ok(expr)
    }

    /// Parse dimension content inside brackets: `M L T^-2]` or `L/T]`
    /// The opening `[` has already been consumed.
    fn parse_dimension_bracket(&mut self) -> Result<Dimension, ParseError> {
        use crate::dim::BaseDim;

        let mut dim = Dimension::dimensionless();
        let mut divisor = false;

        loop {
            match self.peek() {
                Some(Token::RBracket) => {
                    self.next();
                    break;
                }
                Some(Token::Slash) => {
                    self.next();
                    divisor = true;
                }
                // Allow `1` as a no-op numerator: [1] is dimensionless, [1/T] = [T^-1]
                Some(Token::Number(s)) if s == "1" => {
                    self.next();
                }
                Some(Token::Ident(_)) => {
                    let name = match self.next() {
                        Some(Token::Ident(s)) => s,
                        _ => unreachable!(),
                    };
                    let base = BaseDim::from_str(&name).ok_or_else(|| {
                        if crate::unit::lookup_unit(&name).is_some() {
                            ParseError::Expected(format!(
                                "dimension (e.g., L, M, T), not unit '{}'. \
                                 Variables require dimension annotations, not units",
                                name
                            ))
                        } else {
                            ParseError::Expected(format!("base dimension, got '{}'", name))
                        }
                    })?;
                    // Check for ^exponent
                    let mut exp: i32 = 1;
                    if matches!(self.peek(), Some(Token::Caret)) {
                        self.next(); // consume ^
                        let neg = if matches!(self.peek(), Some(Token::Minus)) {
                            self.next();
                            true
                        } else {
                            false
                        };
                        let exp_str = match self.next() {
                            Some(Token::Number(s)) => s,
                            _ => return Err(ParseError::Expected("exponent number".to_string())),
                        };
                        exp = exp_str.parse().map_err(|_| {
                            ParseError::InvalidNumber(exp_str)
                        })?;
                        if neg {
                            exp = -exp;
                        }
                    }
                    if divisor {
                        exp = -exp;
                    }
                    dim = dim.mul(&Dimension::single(base, exp));
                    divisor = false;
                }
                _ => return Err(ParseError::Expected("]".to_string())),
            }
        }

        Ok(dim)
    }

    /// Parse unit content inside brackets: `m/s]` or `kg*m/s^2]`
    /// The opening `[` has already been consumed.
    fn parse_unit_bracket(&mut self) -> Result<Unit, ParseError> {
        use crate::unit::lookup_unit;

        let mut dimension = Dimension::dimensionless();
        let mut scale: f64 = 1.0;
        let mut display = String::new();
        let mut divisor = false;

        loop {
            match self.peek() {
                Some(Token::RBracket) => {
                    self.next();
                    break;
                }
                Some(Token::Slash) => {
                    self.next();
                    display.push('/');
                    divisor = true;
                }
                Some(Token::Star) => {
                    self.next();
                    if !display.is_empty() {
                        display.push('*');
                    }
                }
                // Allow `1` as a no-op numerator: [1/s] = [s^-1]
                Some(Token::Number(s)) if s == "1" => {
                    self.next();
                }
                Some(Token::Ident(_)) => {
                    let name = match self.next() {
                        Some(Token::Ident(s)) => s,
                        _ => unreachable!(),
                    };

                    let (unit_dim, unit_scale) = match lookup_unit(&name) {
                        Some(result) => result,
                        None => {
                            if crate::dim::BaseDim::from_str(&name).is_some() {
                                return Err(ParseError::Expected(format!(
                                    "unit (e.g., m, s, kg), not dimension '{}'. \
                                     Numeric values require units, not dimensions",
                                    name
                                )));
                            }
                            return Err(ParseError::Expected(format!(
                                "unit symbol, got '{}'",
                                name
                            )));
                        }
                    };

                    if !display.is_empty()
                        && !display.ends_with('/')
                        && !display.ends_with('*')
                    {
                        display.push(' ');
                    }
                    display.push_str(&name);

                    // Check for ^exponent
                    let mut exp: i32 = 1;
                    if matches!(self.peek(), Some(Token::Caret)) {
                        self.next(); // consume ^
                        let neg = if matches!(self.peek(), Some(Token::Minus)) {
                            self.next();
                            true
                        } else {
                            false
                        };
                        let exp_str = match self.next() {
                            Some(Token::Number(s)) => s,
                            _ => {
                                return Err(ParseError::Expected("exponent number".to_string()))
                            }
                        };
                        let exp_val: i32 = exp_str.parse().map_err(|_| {
                            ParseError::InvalidNumber(exp_str.clone())
                        })?;
                        exp = if neg { -exp_val } else { exp_val };
                        if neg {
                            display.push_str(&format!("^-{}", exp_str));
                        } else {
                            display.push_str(&format!("^{}", exp_str));
                        }
                    }

                    // Apply divisor sign flip
                    let effective_exp = if divisor { -exp } else { exp };

                    dimension = dimension.mul(&unit_dim.pow(effective_exp));
                    scale *= unit_scale.powi(effective_exp);

                    divisor = false;
                }
                _ => return Err(ParseError::Expected("]".to_string())),
            }
        }

        Ok(Unit {
            dimension,
            scale,
            display,
        })
    }

    fn parse_brace_index_list(
        &mut self,
        position: IndexPosition,
    ) -> Result<Vec<Index>, ParseError> {
        let mut indices = Vec::new();
        loop {
            let ident = match self.next() {
                Some(Token::Ident(name)) => name,
                _ => return Err(ParseError::InvalidIndexList),
            };
            indices.push(Index {
                name: ident,
                position: position.clone(),
            });
            match self.peek() {
                Some(Token::Comma) => {
                    self.next();
                    continue;
                }
                Some(Token::RBrace) => {
                    self.next();
                    break;
                }
                _ => return Err(ParseError::InvalidIndexList),
            }
        }
        Ok(indices)
    }

    fn next_starts_primary(&self) -> bool {
        matches!(
            self.peek(),
            Some(Token::Number(_)) | Some(Token::Ident(_)) | Some(Token::LParen) | Some(Token::Pi)
        )
    }
}

/// Check if an expression contains any variable references.
fn expr_has_vars(expr: &Expr) -> bool {
    match expr {
        Expr::Var { .. } => true,
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            expr_has_vars(a) || expr_has_vars(b)
        }
        Expr::Neg(a) | Expr::Inv(a) | Expr::Fn(_, a) => expr_has_vars(a),
        Expr::FnN(_, args) => args.iter().any(expr_has_vars),
        Expr::Rational(_) | Expr::FracPi(_) | Expr::Named(_) => false,
        Expr::Quantity(inner, _) => expr_has_vars(inner),
    }
}

fn is_known_function(name: &str) -> bool {
    matches!(
        name,
        "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "exp"
            | "ln"
            | "sign"
            | "floor"
            | "ceil"
            | "round"
            | "sqrt"
            | "min"
            | "max"
            | "clamp"
    )
}

fn expect_arity(
    args: Vec<crate::expr::Expr>,
    n: usize,
    name: &str,
) -> Result<crate::expr::Expr, ParseError> {
    if args.len() != n {
        return Err(ParseError::Expected(format!("{} arity {}", name, n)));
    }
    Ok(args.into_iter().next().unwrap())
}

fn expect_n(
    args: Vec<crate::expr::Expr>,
    n: usize,
    name: &str,
) -> Result<Vec<crate::expr::Expr>, ParseError> {
    if args.len() != n {
        return Err(ParseError::Expected(format!("{} arity {}", name, n)));
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use crate::expr::*;

    #[test]
    fn parse_tensor_indices() {
        let expr = parse_expr("X_{i,j}^{k}").unwrap();
        assert_eq!(expr, tensor("X", vec![lower("i"), lower("j"), upper("k")]));
    }

    #[test]
    fn parse_tensor_lower_only() {
        let expr = parse_expr("v_{p}").unwrap();
        assert_eq!(expr, tensor("v", vec![lower("p")]));
    }

    #[test]
    fn parse_tensor_mixed() {
        let expr = parse_expr("d_{p}^{v}").unwrap();
        assert_eq!(expr, tensor("d", vec![lower("p"), upper("v")]));
    }

    #[test]
    fn parse_coeff_mul() {
        let expr = parse_expr("2x").unwrap();
        assert_eq!(expr, mul(constant(2.0), scalar("x")));
    }

    #[test]
    fn parse_sqrt_and_log_base() {
        let expr = parse_expr("sqrt(x) + log_10(x)").unwrap();
        assert_eq!(
            expr,
            add(
                sqrt(scalar("x")),
                mul(ln(scalar("x")), inv(ln(constant(10.0))))
            )
        );
    }

    #[test]
    fn parse_min_max_clamp() {
        let expr = parse_expr("min(a, b) + max(a, b) + clamp(x, 0, 1)").unwrap();
        assert_eq!(
            expr,
            add(
                add(min(scalar("a"), scalar("b")), max(scalar("a"), scalar("b"))),
                clamp(scalar("x"), constant(0.0), constant(1.0))
            )
        );
    }

    #[test]
    fn parse_nested_functions_and_indices() {
        let expr = parse_expr("sin(A_{i,j}^{k} * cosh(x))").unwrap();
        assert_eq!(
            expr,
            sin(mul(
                tensor("A", vec![lower("i"), lower("j"), upper("k")]),
                cosh(scalar("x"))
            ))
        );
    }

    #[test]
    fn parse_log_with_expression_base() {
        let expr = parse_expr("log_(a + b)(x)").unwrap();
        assert_eq!(
            expr,
            mul(ln(scalar("x")), inv(ln(add(scalar("a"), scalar("b")))))
        );
    }

    #[test]
    fn parse_precedence_and_power() {
        let expr = parse_expr("2x^2 + -y").unwrap();
        assert_eq!(
            expr,
            add(
                mul(constant(2.0), pow(scalar("x"), constant(2.0))),
                neg(scalar("y"))
            )
        );
    }

    #[test]
    fn parse_implicit_mul_with_parens() {
        let expr = parse_expr("2(x + 1)").unwrap();
        assert_eq!(expr, mul(constant(2.0), add(scalar("x"), constant(1.0))));
    }

    #[test]
    fn parse_implicit_mul_ident_parens() {
        let expr = parse_expr("x(y + 1)").unwrap();
        assert_eq!(expr, mul(scalar("x"), add(scalar("y"), constant(1.0))));
    }

    #[test]
    fn parse_unknown_ident_parens_as_mul() {
        let expr = parse_expr("foo(x + 1)").unwrap();
        assert_eq!(expr, mul(scalar("foo"), add(scalar("x"), constant(1.0))));
    }

    #[test]
    fn parse_known_function_not_mul() {
        let expr = parse_expr("sin(x + 1)").unwrap();
        assert_eq!(expr, sin(add(scalar("x"), constant(1.0))));
    }

    #[test]
    fn parse_min_max_nested() {
        let expr = parse_expr("min(max(a, b), c)").unwrap();
        assert_eq!(expr, min(max(scalar("a"), scalar("b")), scalar("c")));
    }

    #[test]
    fn parse_round_trip_fmt() {
        let expr = add(
            mul(constant(2.0), pow(scalar("x"), constant(2.0))),
            clamp(scalar("x"), constant(0.0), constant(1.0)),
        );
        let s = format!("{}", expr);
        let parsed = parse_expr(&s).unwrap();
        assert_eq!(parsed, expr);
    }

    #[test]
    fn parse_unary_minus_vs_subtraction() {
        let expr = parse_expr("-x^2").unwrap();
        assert_eq!(expr, neg(pow(scalar("x"), constant(2.0))));

        let expr = parse_expr("x - -y").unwrap();
        assert_eq!(expr, add(scalar("x"), neg(neg(scalar("y")))));
    }

    #[test]
    fn parse_division_associativity() {
        let expr = parse_expr("a / b / c").unwrap();
        assert_eq!(
            expr,
            mul(mul(scalar("a"), inv(scalar("b"))), inv(scalar("c")))
        );
    }

    #[test]
    fn parse_implicit_mul_chaining() {
        let expr = parse_expr("2x * y").unwrap();
        assert_eq!(expr, mul(mul(constant(2.0), scalar("x")), scalar("y")));
    }

    #[test]
    fn parse_multi_char_indices() {
        let expr = parse_expr("T_{mu,nu}^{alpha}").unwrap();
        assert_eq!(
            expr,
            tensor("T", vec![lower("mu"), lower("nu"), upper("alpha")])
        );
    }

    #[test]
    fn parse_spacing_robustness() {
        let expr = parse_expr("min( a , b ) + X_{ i , j }^{ k }").unwrap();
        assert_eq!(
            expr,
            add(
                min(scalar("a"), scalar("b")),
                tensor("X", vec![lower("i"), lower("j"), upper("k")])
            )
        );
    }

    #[test]
    fn parse_log_base_variants() {
        let expr = parse_expr("log_2(1 + x)").unwrap();
        assert_eq!(
            expr,
            mul(ln(add(constant(1.0), scalar("x"))), inv(ln(constant(2.0))))
        );

        let expr = parse_expr("log_(a+b)(x+y)").unwrap();
        assert_eq!(
            expr,
            mul(
                ln(add(scalar("x"), scalar("y"))),
                inv(ln(add(scalar("a"), scalar("b"))))
            )
        );
    }

    #[test]
    fn parse_error_cases() {
        assert!(parse_expr("log_").is_err());
        assert!(parse_expr("min(a)").is_err());
        assert!(parse_expr("X_{").is_err());
    }

    #[test]
    fn parse_parentheses_precedence() {
        let expr = parse_expr("(x + y) * z").unwrap();
        assert_eq!(expr, mul(add(scalar("x"), scalar("y")), scalar("z")));

        let expr = parse_expr("x * (y + z)").unwrap();
        assert_eq!(expr, mul(scalar("x"), add(scalar("y"), scalar("z"))));

        let expr = parse_expr("-(x + y)^2").unwrap();
        assert_eq!(expr, neg(pow(add(scalar("x"), scalar("y")), constant(2.0))));
    }

    #[test]
    fn parse_tensor_ops() {
        let expr = parse_expr("A_{i} ⋅ B^{i}").unwrap();
        assert_eq!(
            expr,
            mul(tensor("A", vec![lower("i")]), tensor("B", vec![upper("i")]))
        );

        let expr = parse_expr("A^{i} ⊗ B^{j}").unwrap();
        assert_eq!(
            expr,
            mul(tensor("A", vec![upper("i")]), tensor("B", vec![upper("j")]))
        );
    }

    #[test]
    fn parse_pi_symbol() {
        let expr = parse_expr("π").unwrap();
        assert_eq!(expr, frac_pi(1, 1));

        let expr = parse_expr("pi").unwrap();
        assert_eq!(expr, frac_pi(1, 1));

        let expr = parse_expr("2π").unwrap();
        assert_eq!(
            expr,
            mul(Expr::Rational(Rational::from_i64(2)), frac_pi(1, 1))
        );

        let expr = parse_expr("e").unwrap();
        assert_eq!(expr, named(NamedConst::E));
    }

    #[test]
    fn parse_pi_fractions() {
        use crate::rule::RuleSet;
        use crate::search::simplify;

        // pi / 2 should fold to FracPi(1/2)
        let expr = parse_expr("pi / 2").unwrap();
        let simplified = simplify(&expr, &RuleSet::full());
        assert_eq!(simplified, frac_pi(1, 2));

        // pi / 3 should fold to FracPi(1/3)
        let expr = parse_expr("pi / 3").unwrap();
        let simplified = simplify(&expr, &RuleSet::full());
        assert_eq!(simplified, frac_pi(1, 3));

        // 2 * pi should fold to FracPi(2)
        let expr = parse_expr("2 * pi").unwrap();
        let simplified = simplify(&expr, &RuleSet::full());
        assert_eq!(simplified, frac_pi(2, 1));
    }

    #[test]
    fn parse_trig_with_pi_simplifies() {
        use crate::rule::RuleSet;
        use crate::search::simplify;

        // sin(pi/2) = 1
        let expr = parse_expr("sin(pi / 2)").unwrap();
        let simplified = simplify(&expr, &RuleSet::full());
        assert_eq!(simplified, Expr::Rational(Rational::ONE));

        // cos(pi) = -1
        let expr = parse_expr("cos(pi)").unwrap();
        let simplified = simplify(&expr, &RuleSet::full());
        assert_eq!(simplified, Expr::Rational(Rational::NEG_ONE));
    }

    #[test]
    fn parse_implicit_mul_with_function() {
        let expr = parse_expr("2sin(x)").unwrap();
        assert_eq!(expr, mul(constant(2.0), sin(scalar("x"))));
    }

    #[test]
    fn parse_ambiguous_ident_as_var() {
        let expr = parse_expr("sinx").unwrap();
        assert_eq!(expr, scalar("sinx"));
    }

    #[test]
    fn parse_more_error_cases() {
        assert!(parse_expr("min(a, b, c)").is_err());
        assert!(parse_expr("clamp(x, 0)").is_err());
        assert!(parse_expr("log_2x").is_err());
        assert!(parse_expr("x @ y").is_err());
    }

    #[test]
    fn parse_caret_as_exponent() {
        let expr = parse_expr("x^2").unwrap();
        assert_eq!(expr, pow(scalar("x"), constant(2.0)));

        let expr = parse_expr("x^(a+b)").unwrap();
        assert_eq!(
            expr,
            pow(scalar("x"), add(scalar("a"), scalar("b")))
        );

    }

    #[test]
    fn parse_caret_brace_as_index() {
        // ^{} is tensor index, not exponent
        let expr = parse_expr("T^{mu}").unwrap();
        assert_eq!(expr, tensor("T", vec![upper("mu")]));

        let expr = parse_expr("T_{mu}^{nu}").unwrap();
        assert_eq!(
            expr,
            tensor("T", vec![lower("mu"), upper("nu")])
        );
    }

    #[test]
    fn parse_dimension_annotation() {
        use crate::dim::Dimension;

        let expr = parse_expr("v [L T^-1]").unwrap();
        match &expr {
            Expr::Var { name, dim, .. } => {
                assert_eq!(name, "v");
                assert_eq!(dim.as_ref().unwrap(), &Dimension::parse("[L T^-1]").unwrap());
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_dimension_slash_shorthand() {
        use crate::dim::Dimension;

        // [L/T] is shorthand for [L T^-1]
        let expr = parse_expr("v [L/T]").unwrap();
        match &expr {
            Expr::Var { dim, .. } => {
                assert_eq!(dim.as_ref().unwrap(), &Dimension::parse("[L T^-1]").unwrap());
            }
            _ => panic!("expected Var"),
        }

        // [M L/T^2] is shorthand for [M L T^-2]
        let expr = parse_expr("F [M L/T^2]").unwrap();
        match &expr {
            Expr::Var { dim, .. } => {
                assert_eq!(dim.as_ref().unwrap(), &Dimension::parse("[M L T^-2]").unwrap());
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_dimensionless_annotation() {
        let expr = parse_expr("theta [1]").unwrap();
        match &expr {
            Expr::Var { dim, .. } => {
                assert!(dim.as_ref().unwrap().is_dimensionless());
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_dimension_one_over() {
        use crate::dim::Dimension;

        // [1/T] is shorthand for [T^-1]
        let expr = parse_expr("f [1/T]").unwrap();
        match &expr {
            Expr::Var { dim, .. } => {
                assert_eq!(dim.as_ref().unwrap(), &Dimension::parse("[T^-1]").unwrap());
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_quantity_basic() {
        use crate::dim::{BaseDim, Dimension};

        let expr = parse_expr("3 [m]").unwrap();
        match &expr {
            Expr::Quantity(inner, unit) => {
                assert_eq!(**inner, Expr::Rational(Rational::from_i64(3)));
                assert_eq!(unit.dimension, Dimension::single(BaseDim::L, 1));
                assert!((unit.scale - 1.0).abs() < 1e-15);
                assert_eq!(unit.display, "m");
            }
            _ => panic!("expected Quantity, got {:?}", expr),
        }
    }

    #[test]
    fn parse_quantity_speed_of_light() {
        use crate::dim::Dimension;

        let expr = parse_expr("3*10^8 [m/s]").unwrap();
        match &expr {
            Expr::Quantity(_, unit) => {
                assert_eq!(unit.dimension, Dimension::parse("[L T^-1]").unwrap());
                assert!((unit.scale - 1.0).abs() < 1e-15);
            }
            _ => panic!("expected Quantity, got {:?}", expr),
        }
    }

    #[test]
    fn parse_quantity_prefixed_unit() {
        use crate::dim::{BaseDim, Dimension};

        let expr = parse_expr("5 [km]").unwrap();
        match &expr {
            Expr::Quantity(_, unit) => {
                assert_eq!(unit.dimension, Dimension::single(BaseDim::L, 1));
                assert!((unit.scale - 1000.0).abs() < 1e-10);
                assert_eq!(unit.display, "km");
            }
            _ => panic!("expected Quantity"),
        }
    }

    #[test]
    fn parse_quantity_compound_unit() {
        use crate::dim::Dimension;

        let expr = parse_expr("10 [kg*m/s^2]").unwrap();
        match &expr {
            Expr::Quantity(_, unit) => {
                assert_eq!(unit.dimension, Dimension::parse("[M L T^-2]").unwrap());
                assert!((unit.scale - 1.0).abs() < 1e-15);
            }
            _ => panic!("expected Quantity"),
        }
    }

    #[test]
    fn parse_quantity_derived_unit() {
        use crate::dim::Dimension;

        let expr = parse_expr("100 [N]").unwrap();
        match &expr {
            Expr::Quantity(_, unit) => {
                assert_eq!(unit.dimension, Dimension::parse("[M L T^-2]").unwrap());
                assert!((unit.scale - 1.0).abs() < 1e-15);
                assert_eq!(unit.display, "N");
            }
            _ => panic!("expected Quantity"),
        }
    }

    #[test]
    fn parse_quantity_one_over_unit() {
        use crate::dim::Dimension;

        let expr = parse_expr("5 [1/s]").unwrap();
        match &expr {
            Expr::Quantity(_, unit) => {
                assert_eq!(unit.dimension, Dimension::parse("[T^-1]").unwrap());
            }
            _ => panic!("expected Quantity"),
        }
    }

    #[test]
    fn parse_quantity_addition() {
        let expr = parse_expr("3 [m] + 5 [m]").unwrap();
        assert!(matches!(&expr, Expr::Add(_, _)));
    }

    #[test]
    fn parse_quantity_round_trip() {
        // Parsing a Quantity's display format should produce the same expression
        let expr = parse_expr("3 [m]").unwrap();
        let displayed = format!("{}", expr);
        assert_eq!(displayed, "3 [m]");
        let reparsed = parse_expr(&displayed).unwrap();
        assert_eq!(reparsed, expr);
    }

    #[test]
    fn parse_var_with_unit_is_error() {
        // Variable + unit annotation should fail
        assert!(parse_expr("c [m/s]").is_err());
    }

    #[test]
    fn parse_number_with_dim_is_error() {
        // Number + dimension annotation should fail (leftover bracket)
        assert!(parse_expr("3 [L]").is_err());
    }

    #[test]
    fn parse_unknown_unit_is_error() {
        // Unknown unit symbol should fail
        assert!(parse_expr("3000 [kg/zm^3]").is_err());
    }
}
