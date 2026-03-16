use crate::dim::Dimension;
use crate::expr::{
    acos, add, asin, atan, ceil, clamp, constant, cos, cosh, exp, floor, frac_pi, inv, ln, max,
    min, mul, named, neg, pow, quantity, round, scalar, sign, sin, sinh, sqrt, tan, tanh, Expr,
    ExprKind, FnKind, Index, IndexPosition, NamedConst, Span,
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

fn tokenize(src: &str) -> Result<Vec<(Token, Span)>, ParseError> {
    let mut chars = src.chars().peekable();
    let mut tokens = Vec::new();
    let mut pos: usize = 0;

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            pos += 1;
            continue;
        }

        if ch.is_ascii_digit() || ch == '.' {
            let start = pos;
            let mut s = String::new();
            let mut dot_count = 0;
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    chars.next();
                    pos += 1;
                } else if c == '_' {
                    // Skip visual separators (e.g. 1_000_000)
                    chars.next();
                    pos += 1;
                } else if c == '.' {
                    dot_count += 1;
                    if dot_count > 1 {
                        break;
                    }
                    s.push(c);
                    chars.next();
                    pos += 1;
                } else {
                    break;
                }
            }
            tokens.push((Token::Number(s), Span::new(start, pos)));
            continue;
        }

        if ch.is_ascii_alphabetic() {
            let start = pos;
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() {
                    s.push(c);
                    chars.next();
                    pos += 1;
                } else {
                    break;
                }
            }
            tokens.push((Token::Ident(s), Span::new(start, pos)));
            continue;
        }

        let start = pos;
        let tok = match ch {
            'π' => {
                chars.next();
                pos += 1;
                Token::Pi
            }
            '+' => {
                chars.next();
                pos += 1;
                Token::Plus
            }
            '-' => {
                chars.next();
                pos += 1;
                Token::Minus
            }
            '*' => {
                chars.next();
                pos += 1;
                Token::Star
            }
            '/' => {
                chars.next();
                pos += 1;
                Token::Slash
            }
            '(' => {
                chars.next();
                pos += 1;
                Token::LParen
            }
            ')' => {
                chars.next();
                pos += 1;
                Token::RParen
            }
            ',' => {
                chars.next();
                pos += 1;
                Token::Comma
            }
            '_' => {
                chars.next();
                pos += 1;
                Token::Underscore
            }
            '^' => {
                chars.next();
                pos += 1;
                Token::Caret
            }
            '{' => {
                chars.next();
                pos += 1;
                Token::LBrace
            }
            '}' => {
                chars.next();
                pos += 1;
                Token::RBrace
            }
            '[' => {
                chars.next();
                pos += 1;
                Token::LBracket
            }
            ']' => {
                chars.next();
                pos += 1;
                Token::RBracket
            }
            '⊗' => {
                chars.next();
                pos += 1;
                Token::Tensor
            }
            '⋅' => {
                chars.next();
                pos += 1;
                Token::DotOp
            }
            ':' => {
                chars.next();
                pos += 1;
                Token::Colon
            }
            _ => {
                return Err(ParseError::UnexpectedToken(ch.to_string()));
            }
        };
        tokens.push((tok, Span::new(start, pos)));
    }

    Ok(tokens)
}

struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    prev_end: usize,
}

impl Parser {
    fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self {
            tokens,
            pos: 0,
            prev_end: 0,
        }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.pos + offset).map(|(t, _)| t)
    }

    fn next(&mut self) -> Option<Token> {
        if let Some((tok, span)) = self.tokens.get(self.pos).cloned() {
            self.pos += 1;
            self.prev_end = span.end;
            Some(tok)
        } else {
            None
        }
    }

    /// Character offset where the current token starts (or prev_end if at EOF).
    fn start_pos(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|(_, s)| s.start)
            .unwrap_or(self.prev_end)
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
        let start = self.start_pos();
        let mut expr = self.parse_multiplicative()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => {
                    self.next();
                    let rhs = self.parse_multiplicative()?;
                    expr = add(expr, rhs);
                    expr.span = Span::new(start, self.prev_end);
                }
                Some(Token::Minus) => {
                    self.next();
                    let rhs = self.parse_multiplicative()?;
                    expr = add(expr, neg(rhs));
                    expr.span = Span::new(start, self.prev_end);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let start = self.start_pos();
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
                    expr.span = Span::new(start, self.prev_end);
                }
                _ => {
                    // implicit multiplication (e.g. 2x or 2(x+1))
                    if self.next_starts_primary() {
                        let rhs = self.parse_unary()?;
                        expr = mul(expr, rhs);
                        expr.span = Span::new(start, self.prev_end);
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
            expr.span = Span::new(start, self.prev_end);
        }

        Ok(expr)
    }

    fn parse_power(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let start = self.start_pos();
        let mut expr = self.parse_primary()?;
        if matches!(self.peek(), Some(Token::Caret))
            && !matches!(self.peek_at(1), Some(Token::LBrace))
        {
            self.next();
            let rhs = self.parse_power()?;
            expr = pow(expr, rhs);
            expr.span = Span::new(start, self.prev_end);
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<crate::expr::Expr, ParseError> {
        match self.peek() {
            Some(Token::Minus) => {
                let start = self.start_pos();
                self.next();
                let mut expr = neg(self.parse_power()?);
                expr.span = Span::new(start, self.prev_end);
                Ok(expr)
            }
            _ => self.parse_power(),
        }
    }

    fn parse_primary(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let start = self.start_pos();
        match self.next() {
            Some(Token::Number(s)) => {
                let span = Span::new(start, self.prev_end);
                if !s.contains('.') {
                    if let Ok(n) = s.parse::<i64>() {
                        return Ok(Expr::spanned(
                            ExprKind::Rational(Rational::from_i64(n)),
                            span,
                        ));
                    }
                }
                let n: f64 = s
                    .parse()
                    .map_err(|_| ParseError::InvalidNumber(s.clone()))?;
                let mut expr = constant(n);
                expr.span = span;
                Ok(expr)
            }
            Some(Token::Pi) => {
                let mut expr = frac_pi(1, 1);
                expr.span = Span::new(start, self.prev_end);
                Ok(expr)
            }
            Some(Token::Ident(name)) => {
                if name == "pi" {
                    let mut expr = frac_pi(1, 1);
                    expr.span = Span::new(start, self.prev_end);
                    return Ok(expr);
                }
                if name == "e" {
                    let mut expr = named(NamedConst::E);
                    expr.span = Span::new(start, self.prev_end);
                    return Ok(expr);
                }
                if name == "log" && self.peek() == Some(&Token::Underscore) {
                    return self.parse_log_base_tokens(start);
                }
                if name.starts_with("log_") {
                    return self.parse_log_base(name, start);
                }
                // Multi-character names followed by ( are function calls;
                // single-character names are implicit multiplication: x(y+1) = x*(y+1)
                if self.peek() == Some(&Token::LParen) && name.len() > 1 {
                    return self.parse_function_call(name, start);
                }
                let mut expr = scalar(&name);
                expr.span = Span::new(start, self.prev_end);
                expr = self.parse_indices(expr)?;
                Ok(expr)
            }
            Some(Token::LParen) => {
                let expr = self.parse_additive()?;
                self.expect(Token::RParen)?;
                // Span covers the parentheses
                let mut wrapped = expr;
                wrapped.span = Span::new(start, self.prev_end);
                Ok(wrapped)
            }
            Some(tok) => Err(ParseError::UnexpectedToken(format!("{:?}", tok))),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn parse_function_call(
        &mut self,
        name: String,
        start: usize,
    ) -> Result<crate::expr::Expr, ParseError> {
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
        let span = Span::new(start, self.prev_end);

        let mut expr = match name.as_str() {
            "sin" => sin(expect_arity(args, 1, "sin")?),
            "cos" => cos(expect_arity(args, 1, "cos")?),
            "tan" => tan(expect_arity(args, 1, "tan")?),
            "asin" => asin(expect_arity(args, 1, "asin")?),
            "acos" => acos(expect_arity(args, 1, "acos")?),
            "atan" => atan(expect_arity(args, 1, "atan")?),
            "sinh" => sinh(expect_arity(args, 1, "sinh")?),
            "cosh" => cosh(expect_arity(args, 1, "cosh")?),
            "tanh" => tanh(expect_arity(args, 1, "tanh")?),
            "exp" => exp(expect_arity(args, 1, "exp")?),
            "ln" => ln(expect_arity(args, 1, "ln")?),
            "sign" => sign(expect_arity(args, 1, "sign")?),
            "floor" => floor(expect_arity(args, 1, "floor")?),
            "ceil" => ceil(expect_arity(args, 1, "ceil")?),
            "round" => round(expect_arity(args, 1, "round")?),
            "sqrt" => sqrt(expect_arity(args, 1, "sqrt")?),
            "min" => {
                let mut args = expect_n(args, 2, "min")?;
                min(args.remove(0), args.remove(0))
            }
            "max" => {
                let mut args = expect_n(args, 2, "max")?;
                max(args.remove(0), args.remove(0))
            }
            "clamp" => {
                let mut args = expect_n(args, 3, "clamp")?;
                clamp(args.remove(0), args.remove(0), args.remove(0))
            }
            _ => {
                // User-defined function call
                if args.len() == 1 {
                    Expr::new(ExprKind::Fn(
                        FnKind::Custom(name),
                        Box::new(args.into_iter().next().unwrap()),
                    ))
                } else {
                    Expr::new(ExprKind::FnN(FnKind::Custom(name), args))
                }
            }
        };
        expr.span = span;
        Ok(expr)
    }

    fn parse_log_base(
        &mut self,
        name: String,
        start: usize,
    ) -> Result<crate::expr::Expr, ParseError> {
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

        let mut expr = mul(ln(arg), inv(ln(base)));
        expr.span = Span::new(start, self.prev_end);
        Ok(expr)
    }

    fn parse_log_base_tokens(
        &mut self,
        start: usize,
    ) -> Result<crate::expr::Expr, ParseError> {
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
        let mut expr = mul(ln(arg), inv(ln(base)));
        expr.span = Span::new(start, self.prev_end);
        Ok(expr)
    }

    fn parse_indices(
        &mut self,
        mut expr: crate::expr::Expr,
    ) -> Result<crate::expr::Expr, ParseError> {
        let start = expr.span.start;
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
            if let ExprKind::Var { name, .. } = expr.kind {
                expr = Expr::spanned(
                    ExprKind::Var {
                        name,
                        indices: all,
                        dim,
                    },
                    Span::new(start, self.prev_end),
                );
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
                        exp = exp_str
                            .parse()
                            .map_err(|_| ParseError::InvalidNumber(exp_str))?;
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

                    if !display.is_empty() && !display.ends_with('/') && !display.ends_with('*') {
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
                            _ => return Err(ParseError::Expected("exponent number".to_string())),
                        };
                        let exp_val: i32 = exp_str
                            .parse()
                            .map_err(|_| ParseError::InvalidNumber(exp_str.clone()))?;
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
    match &expr.kind {
        ExprKind::Var { .. } => true,
        ExprKind::Add(a, b) | ExprKind::Mul(a, b) | ExprKind::Pow(a, b) => {
            expr_has_vars(a) || expr_has_vars(b)
        }
        ExprKind::Neg(a) | ExprKind::Inv(a) | ExprKind::Fn(_, a) => expr_has_vars(a),
        ExprKind::FnN(_, args) => args.iter().any(expr_has_vars),
        ExprKind::Rational(_) | ExprKind::FracPi(_) | ExprKind::Named(_) => false,
        ExprKind::Quantity(inner, _) => expr_has_vars(inner),
    }
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
    fn parse_unknown_ident_parens_as_func_call() {
        // Multi-character names followed by ( are parsed as function calls
        let expr = parse_expr("foo(x + 1)").unwrap();
        assert_eq!(
            expr,
            Expr::new(ExprKind::Fn(
                FnKind::Custom("foo".to_string()),
                Box::new(add(scalar("x"), constant(1.0)))
            ))
        );
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
            mul(
                Expr::new(ExprKind::Rational(Rational::from_i64(2))),
                frac_pi(1, 1)
            )
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
        assert_eq!(simplified, Expr::new(ExprKind::Rational(Rational::ONE)));

        // cos(pi) = -1
        let expr = parse_expr("cos(pi)").unwrap();
        let simplified = simplify(&expr, &RuleSet::full());
        assert_eq!(simplified, Expr::new(ExprKind::Rational(Rational::NEG_ONE)));
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
        assert_eq!(expr, pow(scalar("x"), add(scalar("a"), scalar("b"))));
    }

    #[test]
    fn parse_caret_brace_as_index() {
        // ^{} is tensor index, not exponent
        let expr = parse_expr("T^{mu}").unwrap();
        assert_eq!(expr, tensor("T", vec![upper("mu")]));

        let expr = parse_expr("T_{mu}^{nu}").unwrap();
        assert_eq!(expr, tensor("T", vec![lower("mu"), upper("nu")]));
    }

    #[test]
    fn parse_dimension_annotation() {
        use crate::dim::Dimension;

        let expr = parse_expr("v [L T^-1]").unwrap();
        match &expr.kind {
            ExprKind::Var { name, dim, .. } => {
                assert_eq!(name, "v");
                assert_eq!(
                    dim.as_ref().unwrap(),
                    &Dimension::parse("[L T^-1]").unwrap()
                );
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_dimension_slash_shorthand() {
        use crate::dim::Dimension;

        // [L/T] is shorthand for [L T^-1]
        let expr = parse_expr("v [L/T]").unwrap();
        match &expr.kind {
            ExprKind::Var { dim, .. } => {
                assert_eq!(
                    dim.as_ref().unwrap(),
                    &Dimension::parse("[L T^-1]").unwrap()
                );
            }
            _ => panic!("expected Var"),
        }

        // [M L/T^2] is shorthand for [M L T^-2]
        let expr = parse_expr("F [M L/T^2]").unwrap();
        match &expr.kind {
            ExprKind::Var { dim, .. } => {
                assert_eq!(
                    dim.as_ref().unwrap(),
                    &Dimension::parse("[M L T^-2]").unwrap()
                );
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_dimensionless_annotation() {
        let expr = parse_expr("theta [1]").unwrap();
        match &expr.kind {
            ExprKind::Var { dim, .. } => {
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
        match &expr.kind {
            ExprKind::Var { dim, .. } => {
                assert_eq!(dim.as_ref().unwrap(), &Dimension::parse("[T^-1]").unwrap());
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_quantity_basic() {
        use crate::dim::{BaseDim, Dimension};

        let expr = parse_expr("3 [m]").unwrap();
        match &expr.kind {
            ExprKind::Quantity(inner, unit) => {
                assert_eq!(
                    **inner,
                    Expr::new(ExprKind::Rational(Rational::from_i64(3)))
                );
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
        match &expr.kind {
            ExprKind::Quantity(_, unit) => {
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
        match &expr.kind {
            ExprKind::Quantity(_, unit) => {
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
        match &expr.kind {
            ExprKind::Quantity(_, unit) => {
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
        match &expr.kind {
            ExprKind::Quantity(_, unit) => {
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
        match &expr.kind {
            ExprKind::Quantity(_, unit) => {
                assert_eq!(unit.dimension, Dimension::parse("[T^-1]").unwrap());
            }
            _ => panic!("expected Quantity"),
        }
    }

    #[test]
    fn parse_quantity_addition() {
        let expr = parse_expr("3 [m] + 5 [m]").unwrap();
        assert!(matches!(&expr.kind, ExprKind::Add(_, _)));
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

    #[test]
    fn parse_underscore_in_number() {
        let e = parse_expr("1_000_000").unwrap();
        assert_eq!(e, constant(1_000_000.0));
    }

    #[test]
    fn parse_underscore_in_decimal() {
        let e = parse_expr("1_000.5").unwrap();
        assert_eq!(e, constant(1_000.5));
    }

    #[test]
    fn parse_underscore_in_expression() {
        let e = parse_expr("2 * 1_000").unwrap();
        assert_eq!(e, mul(constant(2.0), constant(1_000.0)));
    }

    // --- Span tests ---

    #[test]
    fn span_number() {
        let e = parse_expr("42").unwrap();
        assert_eq!(e.span, Span::new(0, 2));
    }

    #[test]
    fn span_variable() {
        let e = parse_expr("x").unwrap();
        assert_eq!(e.span, Span::new(0, 1));
    }

    #[test]
    fn span_addition() {
        //                  0123456
        let e = parse_expr("x + 42").unwrap();
        assert_eq!(e.span, Span::new(0, 6));
        // lhs: x at 0..1
        if let ExprKind::Add(a, b) = &e.kind {
            assert_eq!(a.span, Span::new(0, 1));
            assert_eq!(b.span, Span::new(4, 6));
        } else {
            panic!("expected Add");
        }
    }

    #[test]
    fn span_quantity() {
        //                  0123456
        let e = parse_expr("3 [m/s]").unwrap();
        assert_eq!(e.span, Span::new(0, 7));
    }

    #[test]
    fn span_power_with_unit() {
        //                  01234567890123456789
        let e = parse_expr("3 * 10 ^ (8 [m/s])").unwrap();
        // The whole expression
        assert_eq!(e.span, Span::new(0, 18));
        // Dig into: Mul(3, Pow(10, Quantity(8, m/s)))
        if let ExprKind::Mul(lhs, rhs) = &e.kind {
            assert_eq!(lhs.span, Span::new(0, 1)); // "3"
                                                   // rhs is 10 ^ (8 [m/s])
            assert_eq!(rhs.span, Span::new(4, 18)); // "10 ^ (8 [m/s])"
            if let ExprKind::Pow(base, exp) = &rhs.kind {
                assert_eq!(base.span, Span::new(4, 6)); // "10"
                                                        // exp is (8 [m/s]) — parens set the span
                assert_eq!(exp.span, Span::new(9, 18)); // "(8 [m/s])"
                                                        // inner of parens: Quantity(8, m/s)
                if let ExprKind::Quantity(inner, _) = &exp.kind {
                    assert_eq!(inner.span, Span::new(10, 11)); // "8"
                } else {
                    panic!("expected Quantity");
                }
            } else {
                panic!("expected Pow");
            }
        } else {
            panic!("expected Mul");
        }
    }

    #[test]
    fn span_function_call() {
        //                  0123456
        let e = parse_expr("sin(x)").unwrap();
        assert_eq!(e.span, Span::new(0, 6));
    }

    #[test]
    fn span_var_with_dimension() {
        //                  0123456789
        let e = parse_expr("v [L T^-1]").unwrap();
        assert_eq!(e.span, Span::new(0, 10));
    }
}
