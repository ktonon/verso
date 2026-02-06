use crate::expr::{
    acos, add, asin, atan, ceil, clamp, constant, cos, cosh, exp, floor, inv, ln, max, min, mul,
    neg, pow, round, scalar, sign, sin, sinh, sqrt, tan, tanh, tensor, Index, IndexPosition,
};

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
    Plus,
    Minus,
    Star,
    Slash,
    Pow,
    LParen,
    RParen,
    Comma,
    Underscore,
    Caret,
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

        if ch.is_ascii_alphabetic() || ch == '_' {
            if ch == '_' && chars.clone().nth(1) == Some('(') {
                chars.next();
                tokens.push(Token::Underscore);
                continue;
            }
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c == '_' && chars.clone().nth(1) == Some('(') {
                    break;
                }
                if c.is_ascii_alphanumeric() || c == '_' {
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
                if chars.peek() == Some(&'*') {
                    chars.next();
                    tokens.push(Token::Pow);
                } else {
                    tokens.push(Token::Star);
                }
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
            '^' => {
                chars.next();
                tokens.push(Token::Caret);
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
                Some(Token::Star) | Some(Token::Slash) | Some(Token::Tensor) | Some(Token::DotOp)
                | Some(Token::Colon) => {
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

        Ok(expr)
    }

    fn parse_power(&mut self) -> Result<crate::expr::Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        if matches!(self.peek(), Some(Token::Pow)) {
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
                let n: f64 = s
                    .parse()
                    .map_err(|_| ParseError::InvalidNumber(s.clone()))?;
                Ok(constant(n))
            }
            Some(Token::Ident(name)) => {
                if name == "log" && self.peek() == Some(&Token::Underscore) {
                    return self.parse_log_base_tokens();
                }
                if name.starts_with("log_") {
                    return self.parse_log_base(name);
                }
                if self.peek() == Some(&Token::LParen) {
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
        self.expect(Token::LParen)?;
        let base = self.parse_additive()?;
        self.expect(Token::RParen)?;
        self.expect(Token::LParen)?;
        let arg = self.parse_additive()?;
        self.expect(Token::RParen)?;
        Ok(mul(ln(arg), inv(ln(base))))
    }

    fn parse_indices(&mut self, mut expr: crate::expr::Expr) -> Result<crate::expr::Expr, ParseError> {
        let mut lowers: Vec<Index> = Vec::new();
        let mut uppers: Vec<Index> = Vec::new();

        loop {
            match self.peek() {
                Some(Token::Underscore) => {
                    self.next();
                    let indices = self.parse_index_list(IndexPosition::Lower)?;
                    lowers.extend(indices);
                }
                Some(Token::Caret) => {
                    self.next();
                    let indices = self.parse_index_list(IndexPosition::Upper)?;
                    uppers.extend(indices);
                }
                _ => break,
            }
        }

        if !lowers.is_empty() || !uppers.is_empty() {
            let mut all = Vec::new();
            all.extend(lowers);
            all.extend(uppers);
            if let crate::expr::Expr::Var { name, .. } = expr {
                expr = tensor(&name, all);
            }
        }

        Ok(expr)
    }

    fn parse_index_list(&mut self, position: IndexPosition) -> Result<Vec<Index>, ParseError> {
        self.expect(Token::LParen)?;
        let mut indices = Vec::new();
        loop {
            let ident = match self.next() {
                Some(Token::Ident(name)) => name,
                _ => return Err(ParseError::InvalidIndexList),
            };
            indices.push(Index { name: ident, position: position.clone() });
            match self.peek() {
                Some(Token::Comma) => {
                    self.next();
                    continue;
                }
                Some(Token::RParen) => {
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
            Some(Token::Number(_))
                | Some(Token::Ident(_))
                | Some(Token::LParen)
        )
    }
}

trait SingleArg {
    fn single(self) -> Result<crate::expr::Expr, ParseError>;
}

impl SingleArg for Vec<crate::expr::Expr> {
    fn single(self) -> Result<crate::expr::Expr, ParseError> {
        if self.len() != 1 {
            return Err(ParseError::Expected("arity 1".to_string()));
        }
        Ok(self.into_iter().next().unwrap())
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
    use crate::expr::*;

    #[test]
    fn parse_tensor_indices() {
        let expr = parse_expr("X_(i,j)^(k)").unwrap();
        assert_eq!(
            expr,
            tensor("X", vec![lower("i"), lower("j"), upper("k")])
        );
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
        let expr = parse_expr("sin(A_(i,j)^(k) * cosh(x))").unwrap();
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
            mul(
                ln(scalar("x")),
                inv(ln(add(scalar("a"), scalar("b"))))
            )
        );
    }

    #[test]
    fn parse_precedence_and_power() {
        let expr = parse_expr("2x**2 + -y").unwrap();
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
        assert_eq!(
            expr,
            mul(constant(2.0), add(scalar("x"), constant(1.0)))
        );
    }

    #[test]
    fn parse_min_max_nested() {
        let expr = parse_expr("min(max(a, b), c)").unwrap();
        assert_eq!(
            expr,
            min(max(scalar("a"), scalar("b")), scalar("c"))
        );
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
        let expr = parse_expr("-x**2").unwrap();
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
        assert_eq!(
            expr,
            mul(mul(constant(2.0), scalar("x")), scalar("y"))
        );
    }

    #[test]
    fn parse_multi_char_indices() {
        let expr = parse_expr("T_(mu,nu)^(alpha)").unwrap();
        assert_eq!(
            expr,
            tensor("T", vec![lower("mu"), lower("nu"), upper("alpha")])
        );
    }

    #[test]
    fn parse_spacing_robustness() {
        let expr = parse_expr("min( a , b ) + X_( i , j )^( k )").unwrap();
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
        assert!(parse_expr("X_(").is_err());
    }

    #[test]
    fn parse_parentheses_precedence() {
        let expr = parse_expr("(x + y) * z").unwrap();
        assert_eq!(
            expr,
            mul(add(scalar("x"), scalar("y")), scalar("z"))
        );

        let expr = parse_expr("x * (y + z)").unwrap();
        assert_eq!(
            expr,
            mul(scalar("x"), add(scalar("y"), scalar("z")))
        );

        let expr = parse_expr("-(x + y)**2").unwrap();
        assert_eq!(
            expr,
            neg(pow(add(scalar("x"), scalar("y")), constant(2.0)))
        );
    }

    #[test]
    fn parse_tensor_ops() {
        let expr = parse_expr("A_(i) ⋅ B^(i)").unwrap();
        assert_eq!(
            expr,
            mul(tensor("A", vec![lower("i")]), tensor("B", vec![upper("i")]))
        );

        let expr = parse_expr("A^(i) ⊗ B^(j)").unwrap();
        assert_eq!(
            expr,
            mul(tensor("A", vec![upper("i")]), tensor("B", vec![upper("j")]))
        );
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
}
