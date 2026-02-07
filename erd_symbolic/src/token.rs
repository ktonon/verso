use crate::expr::{Expr, FnKind, IndexPosition, NamedConst};
use crate::rational::Rational;


/// A token in the pre-order serialization of an expression AST.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    // Binary operators
    Add,
    Mul,
    Pow,

    // Unary operators
    Neg,
    Inv,

    // Functions (one per FnKind variant)
    Fn(FnKind),

    // Multi-arg function: followed by Int(arity) token, then arity children
    FnN(FnKind),

    // Variables (De Bruijn indexed by first-appearance order)
    Var(u16),

    // Tensor index markers
    IdxLo,
    IdxHi,
    Idx(u16), // De Bruijn index name

    // Integer values (small range stored directly)
    Int(i64),

    // General rational: next two tokens are Int(num), Int(den)
    Frac,

    // Pi fraction: next two tokens are Int(num), Int(den)
    FracPi,

    // Named constants
    Named(NamedConst),
}

/// Mapping between original variable/index names and De Bruijn indices.
/// Used to reverse the canonicalization when converting tokens back to expressions.
pub struct DeBruijn {
    var_names: Vec<String>,  // index → original name
    idx_names: Vec<String>,  // index → original index name
}

impl DeBruijn {
    fn new() -> Self {
        DeBruijn {
            var_names: Vec::new(),
            idx_names: Vec::new(),
        }
    }

    fn var_id(&mut self, name: &str) -> u16 {
        if let Some(pos) = self.var_names.iter().position(|n| n == name) {
            pos as u16
        } else {
            let id = self.var_names.len() as u16;
            self.var_names.push(name.to_string());
            id
        }
    }

    fn idx_id(&mut self, name: &str) -> u16 {
        if let Some(pos) = self.idx_names.iter().position(|n| n == name) {
            pos as u16
        } else {
            let id = self.idx_names.len() as u16;
            self.idx_names.push(name.to_string());
            id
        }
    }

    /// Get the original variable name for a De Bruijn index.
    pub fn var_name(&self, id: u16) -> Option<&str> {
        self.var_names.get(id as usize).map(|s| s.as_str())
    }

    /// Get the original index name for a De Bruijn index.
    pub fn idx_name(&self, id: u16) -> Option<&str> {
        self.idx_names.get(id as usize).map(|s| s.as_str())
    }

    /// Create a DeBruijn mapping with synthetic names (v0, v1, ... and i0, i1, ...).
    /// Used for validation where original variable names are not available.
    pub fn from_synthetic(num_vars: u16, num_indices: u16) -> Self {
        DeBruijn {
            var_names: (0..num_vars).map(|i| format!("v{}", i)).collect(),
            idx_names: (0..num_indices).map(|i| format!("i{}", i)).collect(),
        }
    }
}

/// Tokenize an expression using pre-order traversal with De Bruijn variable naming.
/// Returns the token sequence and the mapping needed to reverse the canonicalization.
pub fn tokenize(expr: &Expr) -> (Vec<Token>, DeBruijn) {
    let mut tokens = Vec::new();
    let mut db = DeBruijn::new();
    tokenize_rec(expr, &mut tokens, &mut db);
    (tokens, db)
}

fn tokenize_rec(expr: &Expr, tokens: &mut Vec<Token>, db: &mut DeBruijn) {
    match expr {
        Expr::Rational(r) => {
            if r.is_integer() {
                tokens.push(Token::Int(r.num()));
            } else {
                tokens.push(Token::Frac);
                tokens.push(Token::Int(r.num()));
                tokens.push(Token::Int(r.den()));
            }
        }
        Expr::FracPi(r) => {
            tokens.push(Token::FracPi);
            tokens.push(Token::Int(r.num()));
            tokens.push(Token::Int(r.den()));
        }
        Expr::Named(nc) => {
            tokens.push(Token::Named(*nc));
        }
        Expr::Var { name, indices } => {
            let id = db.var_id(name);
            tokens.push(Token::Var(id));
            for idx in indices {
                match idx.position {
                    IndexPosition::Lower => tokens.push(Token::IdxLo),
                    IndexPosition::Upper => tokens.push(Token::IdxHi),
                }
                let idx_id = db.idx_id(&idx.name);
                tokens.push(Token::Idx(idx_id));
            }
        }
        Expr::Add(a, b) => {
            tokens.push(Token::Add);
            tokenize_rec(a, tokens, db);
            tokenize_rec(b, tokens, db);
        }
        Expr::Mul(a, b) => {
            tokens.push(Token::Mul);
            tokenize_rec(a, tokens, db);
            tokenize_rec(b, tokens, db);
        }
        Expr::Pow(a, b) => {
            tokens.push(Token::Pow);
            tokenize_rec(a, tokens, db);
            tokenize_rec(b, tokens, db);
        }
        Expr::Neg(a) => {
            tokens.push(Token::Neg);
            tokenize_rec(a, tokens, db);
        }
        Expr::Inv(a) => {
            tokens.push(Token::Inv);
            tokenize_rec(a, tokens, db);
        }
        Expr::Fn(kind, a) => {
            tokens.push(Token::Fn(kind.clone()));
            tokenize_rec(a, tokens, db);
        }
        Expr::FnN(kind, args) => {
            tokens.push(Token::FnN(kind.clone()));
            tokens.push(Token::Int(args.len() as i64));
            for arg in args {
                tokenize_rec(arg, tokens, db);
            }
        }
    }
}

/// Error type for detokenization failures.
#[derive(Debug, PartialEq)]
pub enum TokenError {
    UnexpectedEnd,
    UnexpectedToken(Token),
    InvalidDeBruijnVar(u16),
    InvalidDeBruijnIdx(u16),
    InvalidArity(i64),
    ExpectedInt(Token),
}

/// Convert a token sequence back into an expression, using the De Bruijn mapping
/// to restore original variable and index names.
pub fn detokenize(tokens: &[Token], db: &DeBruijn) -> Result<Expr, TokenError> {
    let mut pos = 0;
    let result = detokenize_rec(tokens, &mut pos, db)?;
    Ok(result)
}

fn detokenize_rec(tokens: &[Token], pos: &mut usize, db: &DeBruijn) -> Result<Expr, TokenError> {
    if *pos >= tokens.len() {
        return Err(TokenError::UnexpectedEnd);
    }
    let token = &tokens[*pos];
    *pos += 1;
    match token {
        Token::Int(n) => Ok(Expr::Rational(Rational::from_i64(*n))),
        Token::Frac => {
            let num = expect_int(tokens, pos)?;
            let den = expect_int(tokens, pos)?;
            Ok(Expr::Rational(Rational::new(num, den)))
        }
        Token::FracPi => {
            let num = expect_int(tokens, pos)?;
            let den = expect_int(tokens, pos)?;
            Ok(Expr::FracPi(Rational::new(num, den)))
        }
        Token::Named(nc) => Ok(Expr::Named(*nc)),
        Token::Var(id) => {
            let name = db
                .var_name(*id)
                .ok_or(TokenError::InvalidDeBruijnVar(*id))?
                .to_string();
            let mut indices = Vec::new();
            // Consume any following index tokens
            while *pos < tokens.len() {
                match &tokens[*pos] {
                    Token::IdxLo => {
                        *pos += 1;
                        let idx_id = expect_idx(tokens, pos)?;
                        let idx_name = db
                            .idx_name(idx_id)
                            .ok_or(TokenError::InvalidDeBruijnIdx(idx_id))?
                            .to_string();
                        indices.push(crate::expr::Index {
                            name: idx_name,
                            position: IndexPosition::Lower,
                        });
                    }
                    Token::IdxHi => {
                        *pos += 1;
                        let idx_id = expect_idx(tokens, pos)?;
                        let idx_name = db
                            .idx_name(idx_id)
                            .ok_or(TokenError::InvalidDeBruijnIdx(idx_id))?
                            .to_string();
                        indices.push(crate::expr::Index {
                            name: idx_name,
                            position: IndexPosition::Upper,
                        });
                    }
                    _ => break,
                }
            }
            Ok(Expr::Var { name, indices })
        }
        Token::Add => {
            let a = detokenize_rec(tokens, pos, db)?;
            let b = detokenize_rec(tokens, pos, db)?;
            Ok(Expr::Add(Box::new(a), Box::new(b)))
        }
        Token::Mul => {
            let a = detokenize_rec(tokens, pos, db)?;
            let b = detokenize_rec(tokens, pos, db)?;
            Ok(Expr::Mul(Box::new(a), Box::new(b)))
        }
        Token::Pow => {
            let a = detokenize_rec(tokens, pos, db)?;
            let b = detokenize_rec(tokens, pos, db)?;
            Ok(Expr::Pow(Box::new(a), Box::new(b)))
        }
        Token::Neg => {
            let a = detokenize_rec(tokens, pos, db)?;
            Ok(Expr::Neg(Box::new(a)))
        }
        Token::Inv => {
            let a = detokenize_rec(tokens, pos, db)?;
            Ok(Expr::Inv(Box::new(a)))
        }
        Token::Fn(kind) => {
            let a = detokenize_rec(tokens, pos, db)?;
            Ok(Expr::Fn(kind.clone(), Box::new(a)))
        }
        Token::FnN(kind) => {
            let arity = expect_int(tokens, pos)?;
            if arity < 0 {
                return Err(TokenError::InvalidArity(arity));
            }
            let mut args = Vec::new();
            for _ in 0..arity {
                args.push(detokenize_rec(tokens, pos, db)?);
            }
            Ok(Expr::FnN(kind.clone(), args))
        }
        // These should only appear as part of Var/Frac/FracPi sequences
        Token::IdxLo | Token::IdxHi | Token::Idx(_) => {
            Err(TokenError::UnexpectedToken(token.clone()))
        }
    }
}

fn expect_int(tokens: &[Token], pos: &mut usize) -> Result<i64, TokenError> {
    if *pos >= tokens.len() {
        return Err(TokenError::UnexpectedEnd);
    }
    match &tokens[*pos] {
        Token::Int(n) => {
            *pos += 1;
            Ok(*n)
        }
        other => Err(TokenError::ExpectedInt(other.clone())),
    }
}

fn expect_idx(tokens: &[Token], pos: &mut usize) -> Result<u16, TokenError> {
    if *pos >= tokens.len() {
        return Err(TokenError::UnexpectedEnd);
    }
    match &tokens[*pos] {
        Token::Idx(id) => {
            *pos += 1;
            Ok(*id)
        }
        other => Err(TokenError::UnexpectedToken(other.clone())),
    }
}

/// A path from root to a node in the AST, as a sequence of child indices.
/// e.g., `[]` = root, `[0]` = left child of root, `[1, 0]` = left child of right child.
pub type AstPath = Vec<usize>;

/// For each token position, returns `Some(path)` for the first token of an AST node,
/// or `None` for continuation tokens (e.g., the Int tokens after Frac).
pub fn position_to_path(tokens: &[Token]) -> Vec<Option<AstPath>> {
    let mut result = vec![None; tokens.len()];
    let mut pos = 0;
    let path = vec![];
    assign_paths(tokens, &mut pos, &path, &mut result);
    result
}

fn assign_paths(
    tokens: &[Token],
    pos: &mut usize,
    path: &[usize],
    result: &mut Vec<Option<AstPath>>,
) {
    if *pos >= tokens.len() {
        return;
    }
    let start = *pos;
    let token = &tokens[*pos];
    result[start] = Some(path.to_vec());
    *pos += 1;

    match token {
        // Leaf nodes with continuation tokens
        Token::Int(_) | Token::Named(_) => {}
        Token::Frac | Token::FracPi => {
            // Skip two Int continuation tokens
            *pos += 2;
        }
        Token::Var(_) => {
            // Skip any following index tokens
            while *pos < tokens.len() {
                match &tokens[*pos] {
                    Token::IdxLo | Token::IdxHi => {
                        *pos += 1; // skip marker
                        *pos += 1; // skip Idx
                    }
                    _ => break,
                }
            }
        }
        // Binary operators: two children
        Token::Add | Token::Mul | Token::Pow => {
            let mut child_path = path.to_vec();
            child_path.push(0);
            assign_paths(tokens, pos, &child_path, result);
            *child_path.last_mut().unwrap() = 1;
            assign_paths(tokens, pos, &child_path, result);
        }
        // Unary operators: one child
        Token::Neg | Token::Inv | Token::Fn(_) => {
            let mut child_path = path.to_vec();
            child_path.push(0);
            assign_paths(tokens, pos, &child_path, result);
        }
        // Multi-arg function: arity token + children
        Token::FnN(_) => {
            let arity = if let Some(Token::Int(n)) = tokens.get(*pos) {
                *pos += 1;
                *n as usize
            } else {
                return;
            };
            for i in 0..arity {
                let mut child_path = path.to_vec();
                child_path.push(i);
                assign_paths(tokens, pos, &child_path, result);
            }
        }
        // Continuation tokens should not appear at top level
        Token::IdxLo | Token::IdxHi | Token::Idx(_) => {}
    }
}

/// Find the token position corresponding to a given AST path.
pub fn path_to_position(tokens: &[Token], target: &AstPath) -> Option<usize> {
    let paths = position_to_path(tokens);
    paths
        .iter()
        .position(|p| p.as_ref() == Some(target))
}

/// Get a reference to the sub-expression at the given AST path.
pub fn subexpr_at<'a>(expr: &'a Expr, path: &AstPath) -> Option<&'a Expr> {
    let mut current = expr;
    for &child_idx in path {
        current = match (current, child_idx) {
            (Expr::Add(a, _) | Expr::Mul(a, _) | Expr::Pow(a, _), 0) => a,
            (Expr::Add(_, b) | Expr::Mul(_, b) | Expr::Pow(_, b), 1) => b,
            (Expr::Neg(a) | Expr::Inv(a) | Expr::Fn(_, a), 0) => a,
            (Expr::FnN(_, args), i) => args.get(i)?,
            _ => return None,
        };
    }
    Some(current)
}

/// Replace the sub-expression at the given AST path with a new expression.
/// Returns the modified expression, or None if the path is invalid.
pub fn replace_subexpr(expr: &Expr, path: &[usize], replacement: Expr) -> Option<Expr> {
    if path.is_empty() {
        return Some(replacement);
    }
    let child_idx = path[0];
    let rest = &path[1..];
    match (expr, child_idx) {
        (Expr::Add(a, b), 0) => {
            let new_a = replace_subexpr(a, rest, replacement)?;
            Some(Expr::Add(Box::new(new_a), b.clone()))
        }
        (Expr::Add(a, b), 1) => {
            let new_b = replace_subexpr(b, rest, replacement)?;
            Some(Expr::Add(a.clone(), Box::new(new_b)))
        }
        (Expr::Mul(a, b), 0) => {
            let new_a = replace_subexpr(a, rest, replacement)?;
            Some(Expr::Mul(Box::new(new_a), b.clone()))
        }
        (Expr::Mul(a, b), 1) => {
            let new_b = replace_subexpr(b, rest, replacement)?;
            Some(Expr::Mul(a.clone(), Box::new(new_b)))
        }
        (Expr::Pow(a, b), 0) => {
            let new_a = replace_subexpr(a, rest, replacement)?;
            Some(Expr::Pow(Box::new(new_a), b.clone()))
        }
        (Expr::Pow(a, b), 1) => {
            let new_b = replace_subexpr(b, rest, replacement)?;
            Some(Expr::Pow(a.clone(), Box::new(new_b)))
        }
        (Expr::Neg(a), 0) => {
            let new_a = replace_subexpr(a, rest, replacement)?;
            Some(Expr::Neg(Box::new(new_a)))
        }
        (Expr::Inv(a), 0) => {
            let new_a = replace_subexpr(a, rest, replacement)?;
            Some(Expr::Inv(Box::new(new_a)))
        }
        (Expr::Fn(kind, a), 0) => {
            let new_a = replace_subexpr(a, rest, replacement)?;
            Some(Expr::Fn(kind.clone(), Box::new(new_a)))
        }
        (Expr::FnN(kind, args), i) if i < args.len() => {
            let mut new_args = args.clone();
            new_args[i] = replace_subexpr(&args[i], rest, replacement)?;
            Some(Expr::FnN(kind.clone(), new_args))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::*;

    // ---- Tokenize + Detokenize roundtrip ----

    #[test]
    fn roundtrip_integer() {
        let expr = rational(3, 1);
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::Int(3)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_fraction() {
        let expr = rational(3, 7);
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::Frac, Token::Int(3), Token::Int(7)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_frac_pi() {
        let expr = frac_pi(1, 4);
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::FracPi, Token::Int(1), Token::Int(4)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_named() {
        let expr = named(NamedConst::E);
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::Named(NamedConst::E)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_variable() {
        let expr = scalar("x");
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::Var(0)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_add() {
        let expr = add(scalar("x"), rational(2, 1));
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::Add, Token::Var(0), Token::Int(2)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_nested() {
        // x**2 + 2*x + 1
        let expr = add(
            add(pow(scalar("x"), rational(2, 1)), mul(rational(2, 1), scalar("x"))),
            rational(1, 1),
        );
        let (tokens, db) = tokenize(&expr);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_neg_inv() {
        let expr = neg(inv(scalar("x")));
        let (tokens, db) = tokenize(&expr);
        assert_eq!(
            tokens,
            vec![Token::Neg, Token::Inv, Token::Var(0)]
        );
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_fn() {
        let expr = sin(scalar("x"));
        let (tokens, db) = tokenize(&expr);
        assert_eq!(tokens, vec![Token::Fn(FnKind::Sin), Token::Var(0)]);
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_fnn() {
        let expr = min(scalar("a"), scalar("b"));
        let (tokens, db) = tokenize(&expr);
        assert_eq!(
            tokens,
            vec![
                Token::FnN(FnKind::Min),
                Token::Int(2),
                Token::Var(0),
                Token::Var(1),
            ]
        );
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_tensor() {
        let expr = tensor("g", vec![lower("mu"), lower("nu")]);
        let (tokens, db) = tokenize(&expr);
        assert_eq!(
            tokens,
            vec![
                Token::Var(0),
                Token::IdxLo,
                Token::Idx(0),
                Token::IdxLo,
                Token::Idx(1),
            ]
        );
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    #[test]
    fn roundtrip_tensor_mixed_indices() {
        let expr = tensor("T", vec![lower("i"), upper("j")]);
        let (tokens, db) = tokenize(&expr);
        assert_eq!(
            tokens,
            vec![
                Token::Var(0),
                Token::IdxLo,
                Token::Idx(0),
                Token::IdxHi,
                Token::Idx(1),
            ]
        );
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }

    // ---- De Bruijn canonicalization ----

    #[test]
    fn de_bruijn_variable_naming() {
        // x*y + y*x and a*b + b*a should produce identical tokens
        let expr1 = add(mul(scalar("x"), scalar("y")), mul(scalar("y"), scalar("x")));
        let expr2 = add(mul(scalar("a"), scalar("b")), mul(scalar("b"), scalar("a")));

        let (tokens1, _) = tokenize(&expr1);
        let (tokens2, _) = tokenize(&expr2);

        assert_eq!(tokens1, tokens2);
    }

    #[test]
    fn de_bruijn_preserves_identity() {
        // x*x should have the same Var id for both occurrences
        let expr = mul(scalar("x"), scalar("x"));
        let (tokens, _) = tokenize(&expr);
        assert_eq!(
            tokens,
            vec![Token::Mul, Token::Var(0), Token::Var(0)]
        );
    }

    #[test]
    fn de_bruijn_index_naming() {
        // Tensor indices also get De Bruijn names
        let expr1 = mul(
            tensor("A", vec![upper("i")]),
            tensor("B", vec![lower("i")]),
        );
        let expr2 = mul(
            tensor("X", vec![upper("j")]),
            tensor("Y", vec![lower("j")]),
        );

        let (tokens1, _) = tokenize(&expr1);
        let (tokens2, _) = tokenize(&expr2);

        assert_eq!(tokens1, tokens2);
    }

    // ---- Position mapping ----

    #[test]
    fn position_to_path_simple() {
        // Add(Var(x), Int(2))
        let expr = add(scalar("x"), rational(2, 1));
        let (tokens, _) = tokenize(&expr);
        let paths = position_to_path(&tokens);

        assert_eq!(paths[0], Some(vec![]));       // Add = root
        assert_eq!(paths[1], Some(vec![0]));      // Var(x) = left child
        assert_eq!(paths[2], Some(vec![1]));      // Int(2) = right child
    }

    #[test]
    fn position_to_path_nested() {
        // Add(Mul(V0, V0), Mul(V1, V1))
        let expr = add(mul(scalar("x"), scalar("x")), mul(scalar("y"), scalar("y")));
        let (tokens, _) = tokenize(&expr);
        // tokens: [ADD, MUL, V0, V0, MUL, V1, V1]
        let paths = position_to_path(&tokens);

        assert_eq!(paths[0], Some(vec![]));         // Add = root
        assert_eq!(paths[1], Some(vec![0]));        // Mul = left child of Add
        assert_eq!(paths[2], Some(vec![0, 0]));     // V0 = left child of Mul
        assert_eq!(paths[3], Some(vec![0, 1]));     // V0 = right child of Mul
        assert_eq!(paths[4], Some(vec![1]));        // Mul = right child of Add
        assert_eq!(paths[5], Some(vec![1, 0]));     // V1 = left child of Mul
        assert_eq!(paths[6], Some(vec![1, 1]));     // V1 = right child of Mul
    }

    #[test]
    fn position_to_path_frac_continuation() {
        // Frac(3, 7)
        let expr = rational(3, 7);
        let (tokens, _) = tokenize(&expr);
        // tokens: [FRAC, INT(3), INT(7)]
        let paths = position_to_path(&tokens);

        assert_eq!(paths[0], Some(vec![]));  // Frac = root
        assert_eq!(paths[1], None);          // continuation
        assert_eq!(paths[2], None);          // continuation
    }

    #[test]
    fn path_to_position_roundtrip() {
        let expr = add(mul(scalar("x"), scalar("x")), mul(scalar("y"), scalar("y")));
        let (tokens, _) = tokenize(&expr);

        // The Mul node at path [1] should be at position 4
        assert_eq!(path_to_position(&tokens, &vec![1]), Some(4));
        // The root Add at path [] should be at position 0
        assert_eq!(path_to_position(&tokens, &vec![]), Some(0));
    }

    // ---- subexpr_at and replace_subexpr ----

    #[test]
    fn subexpr_at_root() {
        let expr = add(scalar("x"), scalar("y"));
        assert_eq!(subexpr_at(&expr, &vec![]), Some(&expr));
    }

    #[test]
    fn subexpr_at_children() {
        let x = scalar("x");
        let y = scalar("y");
        let expr = add(x.clone(), y.clone());
        assert_eq!(subexpr_at(&expr, &vec![0]), Some(&x));
        assert_eq!(subexpr_at(&expr, &vec![1]), Some(&y));
    }

    #[test]
    fn subexpr_at_deep() {
        // Add(Mul(x, y), z)  → path [0, 1] should give y
        let x = scalar("x");
        let y = scalar("y");
        let z = scalar("z");
        let expr = add(mul(x, y.clone()), z);
        assert_eq!(subexpr_at(&expr, &vec![0, 1]), Some(&y));
    }

    #[test]
    fn subexpr_at_invalid() {
        let expr = scalar("x");
        assert_eq!(subexpr_at(&expr, &vec![0]), None);
    }

    #[test]
    fn replace_subexpr_root() {
        let expr = scalar("x");
        let result = replace_subexpr(&expr, &vec![], scalar("y")).unwrap();
        assert_eq!(result, scalar("y"));
    }

    #[test]
    fn replace_subexpr_child() {
        let expr = add(scalar("x"), scalar("y"));
        let result = replace_subexpr(&expr, &vec![0], scalar("z")).unwrap();
        assert_eq!(result, add(scalar("z"), scalar("y")));
    }

    #[test]
    fn replace_subexpr_deep() {
        // Add(Mul(x, y), z) → replace [0, 1] with w → Add(Mul(x, w), z)
        let expr = add(mul(scalar("x"), scalar("y")), scalar("z"));
        let result = replace_subexpr(&expr, &vec![0, 1], scalar("w")).unwrap();
        assert_eq!(result, add(mul(scalar("x"), scalar("w")), scalar("z")));
    }

    #[test]
    fn replace_subexpr_invalid() {
        let expr = scalar("x");
        assert!(replace_subexpr(&expr, &vec![0], scalar("y")).is_none());
    }

    // ---- Integration: tokenize → position → subexpr ----

    #[test]
    fn token_position_matches_subexpr() {
        // x*x + y*y: position 5 = Mul(y, y) at path [1]
        let expr = add(mul(scalar("x"), scalar("x")), mul(scalar("y"), scalar("y")));
        let (tokens, _) = tokenize(&expr);
        let paths = position_to_path(&tokens);

        // Position 4 = Mul at path [1]
        let path = paths[4].as_ref().unwrap();
        assert_eq!(path, &vec![1]);

        let sub = subexpr_at(&expr, path).unwrap();
        assert_eq!(*sub, mul(scalar("y"), scalar("y")));
    }

    #[test]
    fn design_doc_example() {
        // From the ML design doc:
        // x*x + y*y + 2*x*y
        // Tokens: [ADD, MUL, V0, V0, ADD, MUL, V1, V1, MUL, MUL, I_2, V0, V1]
        let expr = add(
            mul(scalar("x"), scalar("x")),
            add(
                mul(scalar("y"), scalar("y")),
                mul(mul(rational(2, 1), scalar("x")), scalar("y")),
            ),
        );

        let (tokens, db) = tokenize(&expr);

        assert_eq!(
            tokens,
            vec![
                Token::Add,
                Token::Mul,
                Token::Var(0),
                Token::Var(0),
                Token::Add,
                Token::Mul,
                Token::Var(1),
                Token::Var(1),
                Token::Mul,
                Token::Mul,
                Token::Int(2),
                Token::Var(0),
                Token::Var(1),
            ]
        );

        // Position 5 = the MUL node for y*y
        let paths = position_to_path(&tokens);
        assert_eq!(paths[5], Some(vec![1, 0]));

        let sub = subexpr_at(&expr, &vec![1, 0]).unwrap();
        assert_eq!(*sub, mul(scalar("y"), scalar("y")));

        // Roundtrip
        let result = detokenize(&tokens, &db).unwrap();
        assert_eq!(result, expr);
    }
}
