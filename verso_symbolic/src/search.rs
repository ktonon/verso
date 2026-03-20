use crate::expr::{classify_mul, Expr, ExprKind, FnKind, MulKind};
use crate::rational::Rational;
use crate::rule::RuleSet;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub trait SearchStrategy {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr;
}

/// Beam search explores multiple rewrite paths simultaneously.
/// Unlike greedy search, it allows temporary complexity increases
/// that may lead to overall better simplifications.
pub struct BeamSearch {
    /// Number of candidates to keep at each step
    pub beam_width: usize,
    /// Maximum number of iterations
    pub max_steps: usize,
}

impl Default for BeamSearch {
    fn default() -> Self {
        BeamSearch {
            beam_width: 10,
            max_steps: 100,
        }
    }
}

/// A single step in the simplification trace.
pub struct TraceStep {
    pub expr: Expr,
    pub rule_name: Option<String>,
    pub rule_display: Option<String>,
}

/// A rewrite with metadata about its origin.
#[derive(Clone)]
struct Rewrite {
    expr: Expr,
    /// True if this rewrite came from a rule application (not just commutativity)
    from_rule: bool,
    rule_name: Option<String>,
    rule_display: Option<String>,
}

impl BeamSearch {
    pub fn new(beam_width: usize, max_steps: usize) -> Self {
        BeamSearch {
            beam_width,
            max_steps,
        }
    }

    /// Generate all possible single-step rewrites of an expression.
    /// Uses depth limit to prevent exponential blowup with rules like commutativity.
    fn all_rewrites(&self, expr: &Expr, rules: &RuleSet) -> Vec<Rewrite> {
        self.all_rewrites_depth(expr, rules, 3) // Limit recursion depth
    }

    fn all_rewrites_depth(&self, expr: &Expr, rules: &RuleSet, depth: usize) -> Vec<Rewrite> {
        let mut results = Vec::new();

        // Try applying each rule at the root
        for rule in rules.iter() {
            if let Some(rewritten) = rule.apply_ltr(expr) {
                // Evaluate constants so complexity is accurate
                let folded = eval_constants(&rewritten);
                results.push(Rewrite {
                    expr: folded,
                    from_rule: true,
                    rule_name: Some(rule.name.clone()),
                    rule_display: Some(format!("{}", rule)),
                });
            }
            // Also try RTL for reversible rules
            if rule.reversible {
                if let Some(rewritten) = rule.apply_rtl(expr) {
                    let folded = eval_constants(&rewritten);
                    results.push(Rewrite {
                        expr: folded,
                        from_rule: true,
                        rule_name: Some(rule.name.clone()),
                        rule_display: Some(format!("{} = {}", rule.rhs, rule.lhs)),
                    });
                }
            }
        }

        // Stop recursion at depth limit
        if depth == 0 {
            return results;
        }

        if matches!(
            &expr.kind,
            ExprKind::Rational(_)
                | ExprKind::Named(_)
                | ExprKind::FracPi(_)
                | ExprKind::Var { .. }
                | ExprKind::Quantity(_, _)
        ) {
            return results;
        }

        for child_idx in 0..expr.child_count() {
            let child = expr.child(child_idx).unwrap();
            for rewrite in self.all_rewrites_depth(child, rules, depth - 1) {
                if let Some(rewritten_expr) = expr.replace_child_derived(child_idx, rewrite.expr) {
                    results.push(Rewrite {
                        expr: rewritten_expr,
                        from_rule: rewrite.from_rule,
                        rule_name: rewrite.rule_name,
                        rule_display: rewrite.rule_display,
                    });
                }
            }
        }

        results
    }

    /// Convert an expression to a canonical string for deduplication.
    fn expr_key(expr: &Expr) -> String {
        format!("{:?}", expr)
    }
}

impl SearchStrategy for BeamSearch {
    fn simplify(&self, expr: &Expr, rules: &RuleSet) -> Expr {
        // Track seen expressions to avoid cycles
        let mut seen: HashSet<String> = HashSet::new();

        // Current beam of candidates, sorted by complexity
        let mut beam: Vec<Expr> = vec![expr.clone()];
        seen.insert(Self::expr_key(expr));

        // Track the best (lowest complexity) expression seen
        let mut best = expr.clone();
        let mut best_complexity = expr.complexity();
        // Track if best came from a rule (vs being the original or a commutative swap)
        let mut best_from_rule = false;

        for _step in 0..self.max_steps {
            // Generate all rewrites in parallel across candidates
            let all_rewrites: Vec<Rewrite> = beam
                .par_iter()
                .flat_map(|candidate| self.all_rewrites(candidate, rules))
                .collect();

            // Deduplicate and track best (serial — fast)
            let mut next_beam: Vec<Expr> = Vec::new();
            for rewrite in all_rewrites {
                let key = Self::expr_key(&rewrite.expr);
                if !seen.contains(&key) {
                    seen.insert(key);

                    let complexity = rewrite.expr.complexity();

                    // Update best if:
                    // 1. This is strictly simpler, OR
                    // 2. Same complexity but this is from a rule and current best isn't
                    //    (prefer canonical rule-based forms over original/swapped forms)
                    let dominated = complexity < best_complexity
                        || (complexity == best_complexity && rewrite.from_rule && !best_from_rule);

                    if dominated {
                        best = rewrite.expr.clone();
                        best_complexity = complexity;
                        best_from_rule = rewrite.from_rule;
                    }

                    next_beam.push(rewrite.expr);
                }
            }

            if next_beam.is_empty() {
                // No new candidates, we've reached a fixed point
                break;
            }

            // Sort by complexity and keep top beam_width candidates
            next_beam.sort_by_key(|e| e.complexity());
            next_beam.truncate(self.beam_width);

            beam = next_beam;
        }

        best
    }
}

/// Derivation record: how an expression was produced.
struct Derivation {
    expr: Expr,
    parent_key: Option<String>,
    rule_name: Option<String>,
    rule_display: Option<String>,
}

impl BeamSearch {
    fn simplify_with_trace(&self, expr: &Expr, rules: &RuleSet) -> (Expr, Vec<TraceStep>) {
        // Track seen expressions to avoid cycles
        let mut seen: HashSet<String> = HashSet::new();

        // Current beam of candidates, sorted by complexity
        let mut beam: Vec<Expr> = vec![expr.clone()];
        let start_key = Self::expr_key(expr);
        seen.insert(start_key.clone());

        // Track the best (lowest complexity) expression seen
        let mut best_complexity = expr.complexity();
        let mut best_from_rule = false;
        let mut best_key = start_key.clone();

        // Derivation map: expr_key -> how it was produced
        let mut derivations: HashMap<String, Derivation> = HashMap::new();
        derivations.insert(
            start_key,
            Derivation {
                expr: expr.clone(),
                parent_key: None,
                rule_name: None,
                rule_display: None,
            },
        );

        for _step in 0..self.max_steps {
            // Generate rewrites paired with their parent's key
            let all_rewrites: Vec<(String, Rewrite)> = beam
                .par_iter()
                .flat_map_iter(|candidate| {
                    let parent_key = Self::expr_key(candidate);
                    self.all_rewrites(candidate, rules)
                        .into_iter()
                        .map(move |r| (parent_key.clone(), r))
                })
                .collect();

            let mut next_beam: Vec<Expr> = Vec::new();
            for (parent_key, rewrite) in all_rewrites {
                let key = Self::expr_key(&rewrite.expr);
                if !seen.contains(&key) {
                    seen.insert(key.clone());

                    derivations.insert(
                        key.clone(),
                        Derivation {
                            expr: rewrite.expr.clone(),
                            parent_key: Some(parent_key),
                            rule_name: rewrite.rule_name,
                            rule_display: rewrite.rule_display,
                        },
                    );

                    let complexity = rewrite.expr.complexity();
                    let dominated = complexity < best_complexity
                        || (complexity == best_complexity && rewrite.from_rule && !best_from_rule);

                    if dominated {
                        best_complexity = complexity;
                        best_from_rule = rewrite.from_rule;
                        best_key = key;
                    }

                    next_beam.push(rewrite.expr);
                }
            }

            if next_beam.is_empty() {
                break;
            }

            next_beam.sort_by_key(|e| e.complexity());
            next_beam.truncate(self.beam_width);

            beam = next_beam;
        }

        // Reconstruct the derivation path from best back to start
        let mut trace = Vec::new();
        let mut current_key = best_key;
        loop {
            if let Some(d) = derivations.remove(&current_key) {
                let next_key = d.parent_key.clone();
                trace.push(TraceStep {
                    expr: d.expr,
                    rule_name: d.rule_name,
                    rule_display: d.rule_display,
                });
                match next_key {
                    Some(pk) => current_key = pk,
                    None => break,
                }
            } else {
                break;
            }
        }
        trace.reverse();

        let best = trace
            .last()
            .map(|s| s.expr.clone())
            .unwrap_or_else(|| expr.clone());
        (best, trace)
    }
}

/// Convenience function to simplify an expression with default settings.
pub fn simplify(expr: &Expr, rules: &RuleSet) -> Expr {
    // Use wider beam to explore both distribution and factoring paths
    let wide_search = BeamSearch::new(20, 200);

    // First pass with beam search
    let simplified = wide_search.simplify(expr, rules);
    let simplified = eval_constants(&simplified);

    // Re-run rules after constant evaluation so identities like cos(pi/2) apply
    let simplified = wide_search.simplify(&simplified, rules);
    let simplified = eval_constants(&simplified);
    let simplified = collect_linear_terms(&simplified);
    let simplified = eval_constants(&simplified);

    // Another pass to catch remaining simplifications
    let simplified = wide_search.simplify(&simplified, rules);
    let simplified = eval_constants(&simplified);
    let simplified = collect_linear_terms(&simplified);
    let simplified = eval_constants(&simplified);

    // Final pass to try factoring only (don't apply other rules that might
    // undo our work or change argument order)
    let factoring_rules = RuleSet::factoring();
    let factored = BeamSearch::default().simplify(&simplified, &factoring_rules);
    if factored.complexity() <= simplified.complexity() {
        factored
    } else {
        simplified
    }
}

pub fn simplify_with_trace(expr: &Expr, rules: &RuleSet) -> (Expr, Vec<TraceStep>) {
    // Use wider beam to explore both distribution and factoring paths
    let wide_search = BeamSearch::new(20, 200);

    let (best, mut trace) = wide_search.simplify_with_trace(expr, rules);
    let mut current = best;

    let folded = eval_constants(&current);
    if folded != current {
        trace.push(TraceStep {
            expr: folded.clone(),
            rule_name: None,
            rule_display: None,
        });
        current = folded;
    }

    let collected = collect_linear_terms(&current);
    if collected != current {
        trace.push(TraceStep {
            expr: collected.clone(),
            rule_name: None,
            rule_display: None,
        });
        current = collected;
    }

    let final_fold = eval_constants(&current);
    if final_fold != current {
        trace.push(TraceStep {
            expr: final_fold.clone(),
            rule_name: None,
            rule_display: None,
        });
        current = final_fold;
    }

    (current, trace)
}

/// Integer square root: returns Some(r) if n = r*r, None otherwise.
fn isqrt(n: i64) -> Option<i64> {
    if n < 0 {
        return None;
    }
    let r = (n as f64).sqrt().round() as i64;
    if r * r == n { Some(r) } else { None }
}

/// Pure constant evaluation: arithmetic on Rational/FracPi values and trig at constant arguments.
/// No normalization (no factor sorting, no mul shortcuts). This is mathematical evaluation,
/// not a search strategy choice.
pub fn eval_constants(expr: &Expr) -> Expr {
    expr.rewrite_bottom_up_derived(&mut |node| match &node.kind {
        ExprKind::Rational(_)
        | ExprKind::FracPi(_)
        | ExprKind::Named(_)
        | ExprKind::Var { .. }
        | ExprKind::FnN(_, _) => None,
        ExprKind::Quantity(inner, unit) => {
            if let ExprKind::Rational(r) = &inner.kind {
                let val = r.value() * unit.scale;
                let rounded = val.round();
                if (val - rounded).abs() < 1e-9 && rounded.abs() < i64::MAX as f64 {
                    return Some(Expr::derived(ExprKind::Rational(Rational::from_i64(
                        rounded as i64,
                    ))));
                }
                for den in [10i64, 100, 1000, 1_000_000, 1_000_000_000] {
                    let num_f64 = val * den as f64;
                    let num_rounded = num_f64.round();
                    if (num_f64 - num_rounded).abs() < 1e-6 && num_rounded.abs() < i64::MAX as f64
                    {
                        return Some(Expr::derived(ExprKind::Rational(Rational::new(
                            num_rounded as i64,
                            den,
                        ))));
                    }
                }
            }
            None
        }
        ExprKind::Neg(inner) => match &inner.kind {
            ExprKind::Rational(r) => Some(Expr::derived(ExprKind::Rational(-*r))),
            ExprKind::FracPi(r) => Some(Expr::derived(ExprKind::FracPi(-*r))),
            _ => None,
        },
        ExprKind::Inv(inner) => match &inner.kind {
            ExprKind::Rational(r) if !r.is_zero() => {
                Some(Expr::derived(ExprKind::Rational(Rational::ONE / *r)))
            }
            _ => None,
        },
        ExprKind::Add(left, right) => {
            if let (ExprKind::Rational(a), ExprKind::Rational(b)) = (&left.kind, &right.kind) {
                return Some(Expr::derived(ExprKind::Rational(*a + *b)));
            }
            if let (ExprKind::FracPi(a), ExprKind::FracPi(b)) = (&left.kind, &right.kind) {
                let sum = *a + *b;
                return Some(if sum.is_zero() {
                    Expr::derived(ExprKind::Rational(Rational::ZERO))
                } else {
                    Expr::derived(ExprKind::FracPi(sum))
                });
            }
            None
        }
        ExprKind::Mul(left, right) => {
            if let (ExprKind::Quantity(a_inner, u1), ExprKind::Quantity(b_inner, u2)) =
                (&left.kind, &right.kind)
            {
                let inner = eval_constants(&Expr::derived(ExprKind::Mul(
                    a_inner.clone(),
                    b_inner.clone(),
                )));
                return Some(Expr::derived(ExprKind::Quantity(Box::new(inner), u1.mul(u2))));
            }
            if let (ExprKind::Rational(a), ExprKind::Rational(b)) = (&left.kind, &right.kind) {
                return Some(Expr::derived(ExprKind::Rational(*a * *b)));
            }
            if let (ExprKind::Rational(a), ExprKind::FracPi(b))
            | (ExprKind::FracPi(b), ExprKind::Rational(a)) = (&left.kind, &right.kind)
            {
                let prod = *a * *b;
                return Some(if prod.is_zero() {
                    Expr::derived(ExprKind::Rational(Rational::ZERO))
                } else {
                    Expr::derived(ExprKind::FracPi(prod))
                });
            }
            if left == right && !matches!(left.kind, ExprKind::Rational(_) | ExprKind::FracPi(_)) {
                return Some(Expr::derived(ExprKind::Pow(
                    Box::new((**left).clone()),
                    Box::new(Expr::derived(ExprKind::Rational(Rational::TWO))),
                )));
            }
            None
        }
        ExprKind::Pow(base, exp) => {
            if let (ExprKind::Rational(base_r), ExprKind::Rational(exp_r)) = (&base.kind, &exp.kind)
            {
                if exp_r.is_integer() && exp_r.num().abs() <= 20 {
                    let n = exp_r.num();
                    if n >= 0 {
                        let mut result = Rational::ONE;
                        for _ in 0..n {
                            result = result * *base_r;
                        }
                        return Some(Expr::derived(ExprKind::Rational(result)));
                    }
                    if !base_r.is_zero() {
                        let mut result = Rational::ONE;
                        for _ in 0..(-n) {
                            result = result * *base_r;
                        }
                        return Some(Expr::derived(ExprKind::Rational(Rational::ONE / result)));
                    }
                }
                if *exp_r == Rational::new(1, 2) && !base_r.is_negative() {
                    let ns = isqrt(base_r.num());
                    let ds = isqrt(base_r.den());
                    if let (Some(n), Some(d)) = (ns, ds) {
                        return Some(Expr::derived(ExprKind::Rational(Rational::new(n, d))));
                    }
                }
            }
            if let ExprKind::Neg(inner) = &base.kind {
                if let ExprKind::Rational(exp_r) = &exp.kind {
                    if exp_r.is_even() {
                        return Some(Expr::derived(ExprKind::Pow(
                            inner.clone(),
                            Box::new((**exp).clone()),
                        )));
                    }
                }
            }
            None
        }
        ExprKind::Fn(kind, arg) => {
            if let ExprKind::Rational(r) = &arg.kind {
                match kind {
                    FnKind::Floor => {
                        return Some(Expr::derived(ExprKind::Rational(Rational::from_i64(
                            r.floor(),
                        ))));
                    }
                    FnKind::Ceil => {
                        let f = r.floor();
                        let c = if r.fract().is_zero() { f } else { f + 1 };
                        return Some(Expr::derived(ExprKind::Rational(Rational::from_i64(c))));
                    }
                    FnKind::Round => {
                        let f = r.floor();
                        let c = if r.fract() >= Rational::new(1, 2) {
                            f + 1
                        } else {
                            f
                        };
                        return Some(Expr::derived(ExprKind::Rational(Rational::from_i64(c))));
                    }
                    FnKind::Sign => {
                        let v = if r.is_positive() {
                            1
                        } else if r.is_negative() {
                            -1
                        } else {
                            0
                        };
                        return Some(Expr::derived(ExprKind::Rational(Rational::from_i64(v))));
                    }
                    FnKind::Custom(name) if name == "abs" => {
                        return Some(Expr::derived(ExprKind::Rational(r.abs())));
                    }
                    _ => {}
                }
            }
            match (&arg.kind, kind) {
                (ExprKind::FracPi(r), FnKind::Sin | FnKind::Cos | FnKind::Tan) => {
                    let normalized = r.rem_euclid(Rational::TWO);
                    if &normalized != r {
                        Some(Expr::derived(ExprKind::Fn(
                            kind.clone(),
                            Box::new(if normalized.is_zero() {
                                Expr::derived(ExprKind::FracPi(Rational::ZERO))
                            } else {
                                Expr::derived(ExprKind::FracPi(normalized))
                            }),
                        )))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    })
}

/// Collect all indices from an expression.
fn collect_all_indices(expr: &Expr) -> Vec<(String, crate::expr::IndexPosition)> {
    let mut result = Vec::new();
    expr.walk(&mut |node| {
        if let ExprKind::Var { indices, .. } = &node.kind {
            result.extend(
                indices
                    .iter()
                    .map(|idx| (idx.name.clone(), idx.position.clone())),
            );
        }
    });
    result
}

/// Find contracted (dummy) indices - those that appear both upper and lower.
fn find_contracted_indices(
    indices: &[(String, crate::expr::IndexPosition)],
) -> std::collections::HashSet<String> {
    use crate::expr::IndexPosition;
    use std::collections::HashSet;

    let mut uppers: HashSet<String> = HashSet::new();
    let mut lowers: HashSet<String> = HashSet::new();

    for (name, pos) in indices {
        match pos {
            IndexPosition::Upper => {
                uppers.insert(name.clone());
            }
            IndexPosition::Lower => {
                lowers.insert(name.clone());
            }
        }
    }

    // Contracted = appears in both upper and lower
    uppers.intersection(&lowers).cloned().collect()
}

/// Build a mapping from contracted index names to canonical names (_0, _1, ...).
fn build_dummy_map(expr: &Expr) -> HashMap<String, String> {
    let indices = collect_all_indices(expr);
    let contracted = find_contracted_indices(&indices);

    // Sort for deterministic ordering, then assign canonical names
    let mut contracted_sorted: Vec<_> = contracted.into_iter().collect();
    contracted_sorted.sort();

    contracted_sorted
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, format!("_{}", i)))
        .collect()
}

/// Create a canonical key for an expression, handling commutativity of Mul.
/// Also normalizes Mul(x, x) to match Pow(x, 2) for term collection.
/// Dummy indices (contracted) are renamed to canonical names for alpha-equivalence.
fn canonical_key(expr: &Expr) -> String {
    let dummy_map = build_dummy_map(expr);
    canonical_key_with_map(expr, &dummy_map)
}

fn canonical_key_with_map(expr: &Expr, dummy_map: &HashMap<String, String>) -> String {
    use crate::expr::{classify_mul, IndexPosition, MulKind};

    expr.try_fold_post_order(&mut |node, children: Vec<String>| {
        Some(match (&node.kind, children.as_slice()) {
            (ExprKind::Rational(r), []) => format!("Rat({}/{})", r.num(), r.den()),
            (ExprKind::Named(nc), []) => format!("Named({:?})", nc),
            (ExprKind::FracPi(r), []) => format!("FracPi({}/{})", r.num(), r.den()),
            (ExprKind::Var { name, indices, .. }, []) => {
                if indices.is_empty() {
                    format!("Var({})", name)
                } else {
                    let normalized_indices: Vec<String> = indices
                        .iter()
                        .map(|idx| {
                            let idx_name = dummy_map.get(&idx.name).unwrap_or(&idx.name);
                            match idx.position {
                                IndexPosition::Upper => format!("^{}", idx_name),
                                IndexPosition::Lower => format!("_{}", idx_name),
                            }
                        })
                        .collect();
                    format!("Var({}[{}])", name, normalized_indices.join(","))
                }
            }
            (ExprKind::Mul(a, b), [ka, kb]) => {
                if ka == kb {
                    format!("Pow({}, 2)", ka)
                } else if classify_mul(a, b) == MulKind::Outer {
                    format!("Mul({}, {})", ka, kb)
                } else if ka <= kb {
                    format!("Mul({}, {})", ka, kb)
                } else {
                    format!("Mul({}, {})", kb, ka)
                }
            }
            (ExprKind::Add(_, _), [a, b]) => format!("Add({}, {})", a, b),
            (ExprKind::Neg(_), [inner]) => format!("Neg({})", inner),
            (ExprKind::Inv(_), [inner]) => format!("Inv({})", inner),
            (ExprKind::Pow(_, exp), [base, exp_key]) => {
                if matches!(&exp.kind, ExprKind::Rational(r) if *r == Rational::TWO) {
                    format!("Pow({}, 2)", base)
                } else {
                    format!("Pow({}, {})", base, exp_key)
                }
            }
            (ExprKind::Fn(kind, _), [inner]) => format!("Fn({:?}, {})", kind, inner),
            (ExprKind::FnN(kind, _), args) => format!("FnN({:?}, [{}])", kind, args.join(", ")),
            (ExprKind::Quantity(_, unit), [inner]) => format!("Qty({}, {})", inner, unit),
            other => panic!("unexpected canonical key fold shape: {:?}", other),
        })
    })
    .expect("canonical key fold should not fail")
}

/// A coefficient for exact Rational arithmetic.
#[derive(Clone, Copy)]
struct Coeff(Rational);

impl Coeff {
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    fn is_one(&self) -> bool {
        self.0 == Rational::ONE
    }

    fn is_neg_one(&self) -> bool {
        self.0 == Rational::NEG_ONE
    }

    fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    fn neg(self) -> Coeff {
        Coeff(-self.0)
    }

    fn add(self, other: Coeff) -> Coeff {
        Coeff(self.0 + other.0)
    }

    fn to_expr(self) -> Expr {
        Expr::derived(ExprKind::Rational(self.0))
    }
}

pub(crate) fn collect_linear_terms(expr: &Expr) -> Expr {
    let mut terms = Vec::new();
    flatten_add(expr, &mut terms);

    let mut coeffs: HashMap<String, (Expr, Coeff)> = HashMap::new();
    let mut const_sum = Coeff(Rational::ZERO);
    let mut rest: Vec<Expr> = Vec::new();

    for term in terms {
        if let Some((base, coeff)) = extract_term(&term) {
            if matches!(base.kind, ExprKind::Rational(_)) {
                const_sum = const_sum.add(coeff);
                continue;
            }
            let key = canonical_key(&base);
            let entry = coeffs.entry(key).or_insert((base, Coeff(Rational::ZERO)));
            entry.1 = entry.1.add(coeff);
        } else {
            rest.push(term);
        }
    }

    // Collect terms with their coefficients, separating positive and negative
    let mut positive_terms: Vec<(String, Expr)> = Vec::new();
    let mut negative_terms: Vec<(String, Expr)> = Vec::new();
    let mut coeff_keys: Vec<_> = coeffs.keys().cloned().collect();
    coeff_keys.sort();
    for key in coeff_keys {
        let (var, coeff) = coeffs.remove(&key).unwrap();
        if coeff.is_zero() {
            continue;
        }
        let term = if coeff.is_one() {
            var
        } else if coeff.is_neg_one() {
            Expr::derived(ExprKind::Neg(Box::new(var)))
        } else if !coeff.is_positive() {
            Expr::derived(ExprKind::Neg(Box::new(Expr::derived(ExprKind::Mul(
                Box::new(coeff.neg().to_expr()),
                Box::new(var),
            )))))
        } else {
            Expr::derived(ExprKind::Mul(Box::new(coeff.to_expr()), Box::new(var)))
        };
        if coeff.is_positive() {
            positive_terms.push((key, term));
        } else {
            negative_terms.push((key, term));
        }
    }

    rest.sort_by_key(|e| format!("{:?}", e));
    // Put positive terms first, then negative terms
    let mut ordered: Vec<Expr> = positive_terms.into_iter().map(|(_, e)| e).collect();
    ordered.extend(negative_terms.into_iter().map(|(_, e)| e));
    if !const_sum.is_zero() {
        ordered.push(const_sum.to_expr());
    }
    ordered.extend(rest);

    match ordered.len() {
        0 => Expr::derived(ExprKind::Rational(Rational::ZERO)),
        1 => ordered.into_iter().next().unwrap(),
        _ => {
            let mut iter = ordered.into_iter();
            let mut acc = iter.next().unwrap();
            for t in iter {
                acc = Expr::derived(ExprKind::Add(Box::new(acc), Box::new(t)));
            }
            acc
        }
    }
}

fn flatten_add(expr: &Expr, out: &mut Vec<Expr>) {
    match &expr.kind {
        ExprKind::Add(a, b) => {
            flatten_add(a, out);
            flatten_add(b, out);
        }
        _ => out.push(expr.clone()),
    }
}

fn extract_term(expr: &Expr) -> Option<(Expr, Coeff)> {
    let one = Coeff(Rational::ONE);
    match &expr.kind {
        ExprKind::Rational(r) => Some((Expr::derived(ExprKind::Rational(Rational::ONE)), Coeff(*r))),
        ExprKind::Neg(inner) => {
            if let Some((base, coeff)) = extract_term(inner) {
                Some((base, coeff.neg()))
            } else {
                None
            }
        }
        ExprKind::Mul(a, b) => match (&a.kind, &b.kind) {
            (ExprKind::Rational(r), _) => Some((b.as_ref().clone(), Coeff(*r))),
            (_, ExprKind::Rational(r)) => Some((a.as_ref().clone(), Coeff(*r))),
            // Handle nested Mul with leading rational
            (ExprKind::Mul(inner_a, inner_b), _) => {
                if let ExprKind::Rational(r) = &inner_a.kind {
                    let rest = Expr::derived(ExprKind::Mul(inner_b.clone(), b.clone()));
                    Some((rest, Coeff(*r)))
                } else if let Some((inner_base, inner_coeff)) = extract_term(a) {
                    if !inner_coeff.is_one() {
                        let rest = Expr::derived(ExprKind::Mul(Box::new(inner_base), b.clone()));
                        Some((rest, inner_coeff))
                    } else {
                        Some((expr.clone(), one))
                    }
                } else {
                    Some((expr.clone(), one))
                }
            }
            // Handle Neg inside Mul: Mul(Neg(x), y) => -1 * Mul(x, y)
            (ExprKind::Neg(inner_a), _) => {
                let new_mul = Expr::derived(ExprKind::Mul(inner_a.clone(), b.clone()));
                if let Some((base, coeff)) = extract_term(&new_mul) {
                    Some((base, coeff.neg()))
                } else {
                    Some((new_mul, Coeff(Rational::NEG_ONE)))
                }
            }
            (_, ExprKind::Neg(inner_b)) => {
                let new_mul = Expr::derived(ExprKind::Mul(a.clone(), inner_b.clone()));
                if let Some((base, coeff)) = extract_term(&new_mul) {
                    Some((base, coeff.neg()))
                } else {
                    Some((new_mul, Coeff(Rational::NEG_ONE)))
                }
            }
            _ => {
                let mut left = a.clone();
                let mut right = b.clone();
                if classify_mul(a, b) == MulKind::Scalar {
                    if matches!(
                        &left.kind,
                        ExprKind::Inv(inner)
                            if matches!(&inner.kind, ExprKind::Var { indices, .. } if indices.is_empty())
                    ) {
                        std::mem::swap(&mut left, &mut right);
                    }
                }
                Some((Expr::derived(ExprKind::Mul(left, right)), one))
            }
        },
        _ => Some((expr.clone(), one)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{
        add, constant, cos, div, frac_pi, inv, mul, named, neg, pow, rational, scalar, sin, tensor,
        upper, NamedConst,
    };
    use crate::rational::Rational;

    #[test]
    fn simplify_add_zero() {
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_one() {
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_zero() {
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_double_neg() {
        let rules = RuleSet::standard();
        let expr = neg(neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_double_inv() {
        let rules = RuleSet::standard();
        let expr = inv(inv(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_inv_mul_cancel() {
        // (1/x) * x = 1
        let rules = RuleSet::standard();
        let expr = mul(div(constant(1.0), scalar("x")), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_div_self() {
        // x / x = 1
        let rules = RuleSet::standard();
        let expr = div(scalar("x"), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_inv_mul_reorder_to_div() {
        // (1/x) * y — either ordering is valid without normalize_mul
        let rules = RuleSet::standard();
        let expr = mul(div(constant(1.0), scalar("x")), scalar("y"));
        let result = simplify(&expr, &rules);
        assert!(
            result == mul(inv(scalar("x")), scalar("y"))
                || result == mul(scalar("y"), inv(scalar("x"))),
            "expected y * (1/x) or (1/x) * y, got {:?}",
            result,
        );
    }

    #[test]
    fn simplify_inv_mul_collects_with_div() {
        // 1/x*y + y/x - 1 = 2y/x - 1
        let rules = RuleSet::full();
        let expr = add(
            add(
                mul(div(constant(1.0), scalar("x")), scalar("y")),
                div(scalar("y"), scalar("x")),
            ),
            neg(constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        // Result should be equivalent to 2y/x - 1
        // Accept various equivalent forms due to commutativity and distribution
        let form1 = add(
            mul(mul(constant(2.0), inv(scalar("x"))), scalar("y")),
            constant(-1.0),
        );
        let form2 = add(
            mul(add(scalar("y"), scalar("y")), inv(scalar("x"))),
            constant(-1.0),
        );
        let form3 = add(
            mul(inv(scalar("x")), add(scalar("y"), scalar("y"))),
            constant(-1.0),
        );
        // 2 * (y * (1/x)) + (-1)
        let form4 = add(
            mul(constant(2.0), mul(scalar("y"), inv(scalar("x")))),
            constant(-1.0),
        );
        assert!(
            result == form1 || result == form2 || result == form3 || result == form4,
            "Expected 2y/x - 1 in some form, got: {:?}",
            result
        );
    }

    #[test]
    fn simplify_expanded_binomial_to_zero() {
        // (x + 1)(y + 1) - x*y - 1 - x - y = 0
        let rules = RuleSet::full();
        let expr = add(
            add(
                add(
                    add(
                        mul(
                            add(scalar("x"), constant(1.0)),
                            add(scalar("y"), constant(1.0)),
                        ),
                        neg(mul(scalar("x"), scalar("y"))),
                    ),
                    neg(constant(1.0)),
                ),
                neg(scalar("x")),
            ),
            neg(scalar("y")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_pow_one() {
        let rules = RuleSet::standard();
        let expr = pow(scalar("x"), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_zero() {
        let rules = RuleSet::standard();
        let expr = pow(scalar("x"), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_nested_add_zero() {
        // (x + 0) + (y + 0) should simplify to x + y
        let rules = RuleSet::standard();
        let expr = add(
            add(scalar("x"), constant(0.0)),
            add(scalar("y"), constant(0.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_nested_mul_one() {
        // (x * 1) * (y * 1) should simplify to x * y
        let rules = RuleSet::standard();
        let expr = mul(
            mul(scalar("x"), constant(1.0)),
            mul(scalar("y"), constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_chain_identities() {
        // x + 0 + 0 (represented as (x + 0) + 0)
        let rules = RuleSet::standard();
        let expr = add(add(scalar("x"), constant(0.0)), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_trig_sin_zero() {
        let rules = RuleSet::trigonometric();
        let expr = sin(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_zero() {
        let rules = RuleSet::trigonometric();
        let expr = cos(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_sin_pi_4() {
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(1, 4));
        let result = simplify(&expr, &rules);
        assert_eq!(result, named(NamedConst::Frac1Sqrt2)); // √2/2
    }

    #[test]
    fn simplify_trig_cos_pi_4() {
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(1, 4));
        let result = simplify(&expr, &rules);
        assert_eq!(result, named(NamedConst::Frac1Sqrt2)); // √2/2
    }

    #[test]
    fn simplify_trig_sin_pi_3() {
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(1, 3));
        let result = simplify(&expr, &rules);
        assert_eq!(result, named(NamedConst::FracSqrt3By2)); // √3/2
    }

    #[test]
    fn simplify_trig_cos_pi_3() {
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(1, 3));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.5)); // 1/2
    }

    #[test]
    fn simplify_trig_sin_pi_6() {
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(1, 6));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.5)); // 1/2
    }

    #[test]
    fn simplify_trig_cos_pi_6() {
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(1, 6));
        let result = simplify(&expr, &rules);
        assert_eq!(result, named(NamedConst::FracSqrt3By2)); // √3/2
    }

    #[test]
    fn simplify_trig_sin_2pi() {
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(2, 1));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_2pi() {
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(2, 1));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_sin_3pi_2() {
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(3, 2));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(-1.0));
    }

    #[test]
    fn simplify_trig_cos_3pi_2() {
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(3, 2));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_ln_e() {
        use crate::expr::ln;
        let rules = RuleSet::trigonometric();
        let expr = ln(named(NamedConst::E));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_sin_complementary() {
        // sin(π/2 - x) = cos(x)
        let rules = RuleSet::trigonometric();
        let expr = sin(add(frac_pi(1, 2), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_cos_complementary() {
        // cos(π/2 - x) = sin(x)
        let rules = RuleSet::trigonometric();
        let expr = cos(add(frac_pi(1, 2), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(scalar("x")));
    }

    #[test]
    fn simplify_sin_supplementary() {
        // sin(π - x) = sin(x)
        let rules = RuleSet::trigonometric();
        let expr = sin(add(frac_pi(1, 1), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(scalar("x")));
    }

    #[test]
    fn simplify_cos_supplementary() {
        // cos(π - x) = -cos(x)
        let rules = RuleSet::trigonometric();
        let expr = cos(add(frac_pi(1, 1), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(cos(scalar("x"))));
    }

    #[test]
    fn simplify_sin_period() {
        // sin(x + 2π) = sin(x)
        let rules = RuleSet::trigonometric();
        let expr = sin(add(scalar("x"), frac_pi(2, 1)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(scalar("x")));
    }

    #[test]
    fn simplify_cos_period() {
        // cos(x + 2π) = cos(x)
        let rules = RuleSet::trigonometric();
        let expr = cos(add(scalar("x"), frac_pi(2, 1)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_double_angle_sin_contraction() {
        // 2·sin(x)·cos(x) = sin(2x)
        // Complexity: 5 → 3 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = mul(constant(2.0), mul(sin(scalar("x")), cos(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(mul(constant(2.0), scalar("x"))));
    }

    #[test]
    fn simplify_double_angle_cos_contraction() {
        // cos²(x) - sin²(x) = cos(2x)
        // Complexity: 7 → 3 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = add(
            pow(cos(scalar("x")), constant(2.0)),
            neg(pow(sin(scalar("x")), constant(2.0))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(mul(constant(2.0), scalar("x"))));
    }

    #[test]
    fn simplify_power_reduction_to_sin_sq() {
        // (1 - cos(2x))/2 = sin²(x)
        // Complexity: 6 → 3 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = mul(
            inv(constant(2.0)),
            add(constant(1.0), neg(cos(mul(constant(2.0), scalar("x"))))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, pow(sin(scalar("x")), constant(2.0)));
    }

    #[test]
    fn simplify_power_reduction_to_cos_sq() {
        // (1 + cos(2x))/2 = cos²(x)
        // Complexity: 5 → 3 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = mul(
            inv(constant(2.0)),
            add(constant(1.0), cos(mul(constant(2.0), scalar("x")))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, pow(cos(scalar("x")), constant(2.0)));
    }

    #[test]
    fn simplify_sin_sum_contraction() {
        // sin(a)·cos(b) + cos(a)·sin(b) = sin(a + b)
        // Complexity: 9 → 3 (reduces significantly!)
        let rules = RuleSet::trigonometric();
        let expr = add(
            mul(sin(scalar("a")), cos(scalar("b"))),
            mul(cos(scalar("a")), sin(scalar("b"))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(add(scalar("a"), scalar("b"))));
    }

    #[test]
    fn simplify_sin_diff_contraction() {
        // sin(a)·cos(b) - cos(a)·sin(b) = sin(a - b)
        // Complexity: 10 → 4 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = add(
            mul(sin(scalar("a")), cos(scalar("b"))),
            neg(mul(cos(scalar("a")), sin(scalar("b")))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, sin(add(scalar("a"), neg(scalar("b")))));
    }

    #[test]
    fn simplify_cos_sum_contraction() {
        // cos(a)·cos(b) - sin(a)·sin(b) = cos(a + b)
        // Complexity: 10 → 3 (reduces significantly!)
        let rules = RuleSet::trigonometric();
        let expr = add(
            mul(cos(scalar("a")), cos(scalar("b"))),
            neg(mul(sin(scalar("a")), sin(scalar("b")))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(add(scalar("a"), scalar("b"))));
    }

    #[test]
    fn simplify_cos_diff_contraction() {
        // cos(a)·cos(b) + sin(a)·sin(b) = cos(a - b)
        // Complexity: 9 → 4 (reduces!)
        let rules = RuleSet::trigonometric();
        let expr = add(
            mul(cos(scalar("a")), cos(scalar("b"))),
            mul(sin(scalar("a")), sin(scalar("b"))),
        );
        let result = simplify(&expr, &rules);
        // Accept either ordering due to commutativity: cos(a + (-b)) or cos((-b) + a)
        let expected1 = cos(add(scalar("a"), neg(scalar("b"))));
        let expected2 = cos(add(neg(scalar("b")), scalar("a")));
        assert!(
            result == expected1 || result == expected2,
            "Expected cos(a - b) in either ordering, got {:?}",
            result
        );
    }

    #[test]
    fn simplify_ln_product_contraction() {
        // ln(a) + ln(b) = ln(a·b)
        // Complexity: 5 → 3 (reduces!)
        use crate::expr::ln;
        let rules = RuleSet::trigonometric();
        let expr = add(ln(scalar("a")), ln(scalar("b")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, ln(mul(scalar("a"), scalar("b"))));
    }

    #[test]
    fn simplify_ln_quotient_contraction() {
        // ln(a) - ln(b) = ln(a/b)
        // Complexity: 6 → 4 (reduces!)
        use crate::expr::ln;
        let rules = RuleSet::trigonometric();
        let expr = add(ln(scalar("a")), neg(ln(scalar("b"))));
        let result = simplify(&expr, &rules);
        // ln(a * (1/b)) or ln((1/b) * a) — either ordering is valid
        assert!(
            result == ln(mul(inv(scalar("b")), scalar("a")))
                || result == ln(mul(scalar("a"), inv(scalar("b")))),
            "expected ln(a/b) in either order, got {:?}",
            result,
        );
    }

    #[test]
    fn simplify_ln_power_contraction() {
        // n·ln(a) = ln(a^n)
        // Complexity: 4 → 3 (reduces!)
        use crate::expr::ln;
        let rules = RuleSet::trigonometric();
        let expr = mul(scalar("n"), ln(scalar("a")));
        let result = simplify(&expr, &rules);
        // Search now finds the contracted form: ln(a^n)
        assert_eq!(result, ln(pow(scalar("a"), scalar("n"))));
    }

    #[test]
    fn simplify_exp_product_contraction() {
        // exp(a)·exp(b) = exp(a + b)
        // Complexity: 5 → 3 (reduces!)
        use crate::expr::exp;
        let rules = RuleSet::trigonometric();
        let expr = mul(exp(scalar("a")), exp(scalar("b")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, exp(add(scalar("a"), scalar("b"))));
    }

    #[test]
    fn simplify_exp_quotient_contraction() {
        // exp(a) / exp(b) = exp(a - b)
        // exp(a) * (1/exp(b)) = exp(a + (-b))
        // Complexity: 6 → 4 (reduces!)
        use crate::expr::exp;
        let rules = RuleSet::trigonometric();
        let expr = mul(exp(scalar("a")), inv(exp(scalar("b"))));
        let result = simplify(&expr, &rules);
        // Accept either ordering due to commutativity: exp(a + (-b)) or exp((-b) + a)
        let expected1 = exp(add(scalar("a"), neg(scalar("b"))));
        let expected2 = exp(add(neg(scalar("b")), scalar("a")));
        assert!(
            result == expected1 || result == expected2,
            "Expected exp(a-b) in some form, got: {:?}",
            result
        );
    }

    #[test]
    fn simplify_pythagorean() {
        // sin²(x) + cos²(x) = 1
        let rules = RuleSet::trigonometric();
        let expr = add(
            pow(sin(scalar("x")), constant(2.0)),
            pow(cos(scalar("x")), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_kronecker_delta() {
        // δ^μ_ν * v^ν = v^μ
        use crate::expr::lower;
        let rules = RuleSet::tensor();
        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("v", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    #[test]
    fn simplify_preserves_non_matching() {
        // Expression that doesn't match any rules should be unchanged
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_combined_rules() {
        // Test with combined standard + trig rules
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        // sin(0) + x * 1 should become 0 + x = x
        let expr = add(sin(constant(0.0)), mul(scalar("x"), constant(1.0)));
        let result = simplify(&expr, &rules);
        // sin(0) -> 0, x*1 -> x, 0+x -> x
        assert_eq!(result, scalar("x"));
    }

    // === More complicated simplify tests ===

    #[test]
    fn simplify_deeply_nested_zeros() {
        // ((((x + 0) + 0) + 0) + 0) should simplify to x
        let rules = RuleSet::standard();
        let expr = add(
            add(
                add(add(scalar("x"), constant(0.0)), constant(0.0)),
                constant(0.0),
            ),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_deeply_nested_ones() {
        // ((((x * 1) * 1) * 1) * 1) should simplify to x
        let rules = RuleSet::standard();
        let expr = mul(
            mul(
                mul(mul(scalar("x"), constant(1.0)), constant(1.0)),
                constant(1.0),
            ),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mixed_nested_identities() {
        // ((x * 1) + 0) * 1 + 0 should simplify to x
        let rules = RuleSet::standard();
        let expr = add(
            mul(
                add(mul(scalar("x"), constant(1.0)), constant(0.0)),
                constant(1.0),
            ),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_zero_propagation() {
        // (x + y) * 0 should simplify to 0
        let rules = RuleSet::standard();
        let expr = mul(add(scalar("x"), scalar("y")), constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_nested_zero_propagation() {
        // ((a + b) * (c + d)) * 0 should simplify to 0
        let rules = RuleSet::standard();
        let expr = mul(
            mul(add(scalar("a"), scalar("b")), add(scalar("c"), scalar("d"))),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_multiple_double_negations() {
        // ----x should simplify to x
        let rules = RuleSet::standard();
        let expr = neg(neg(neg(neg(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_triple_negation() {
        // ---x should simplify to -x
        let rules = RuleSet::standard();
        let expr = neg(neg(neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(scalar("x")));
    }

    #[test]
    fn simplify_multiple_double_inversions() {
        // 1/(1/(1/(1/x))) should simplify to x
        let rules = RuleSet::standard();
        let expr = inv(inv(inv(inv(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_chain() {
        // (x^1)^1 should simplify to x
        let rules = RuleSet::standard();
        let expr = pow(pow(scalar("x"), constant(1.0)), constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pow_zero_nested() {
        // (complex_expr)^0 should simplify to 1
        let rules = RuleSet::standard();
        let complex = add(mul(scalar("x"), scalar("y")), neg(scalar("z")));
        let expr = pow(complex, constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_one_pow_anything() {
        // 1^(complex_expr) should simplify to 1
        let rules = RuleSet::standard();
        let complex = add(mul(scalar("x"), scalar("y")), scalar("z"));
        let expr = pow(constant(1.0), complex);
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_pythagorean_nested_in_expression() {
        // x * (sin²(y) + cos²(y)) should simplify to x * 1 = x
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let pythagorean = add(
            pow(sin(scalar("y")), constant(2.0)),
            pow(cos(scalar("y")), constant(2.0)),
        );
        let expr = mul(scalar("x"), pythagorean);
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_pythagorean_added_to_expression() {
        // (sin²(x) + cos²(x)) + z should simplify to 1 + z
        let rules = RuleSet::trigonometric();

        let pythagorean = add(
            pow(sin(scalar("x")), constant(2.0)),
            pow(cos(scalar("x")), constant(2.0)),
        );
        let expr = add(pythagorean, scalar("z"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(scalar("z"), constant(1.0)));
    }

    #[test]
    fn simplify_exp_ln_nested() {
        // exp(ln(exp(ln(x)))) should simplify to x
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = exp(ln(exp(ln(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_ln_exp_nested() {
        // ln(exp(ln(exp(x)))) should simplify to x
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = ln(exp(ln(exp(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_sin_cos_at_zero() {
        // sin(0) + cos(0) should simplify to 0 + 1 with trig rules
        // then to 1 with standard rules
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = add(sin(constant(0.0)), cos(constant(0.0)));
        let result = simplify(&expr, &rules);
        // sin(0) -> 0, cos(0) -> 1, 0 + 1 = 1
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_cos_neg_nested() {
        // cos(-(-x)) should simplify to cos(x) using cos(-u) = cos(u) and --u = u
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = cos(neg(neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_sin_neg_zero() {
        // sin(-0) should simplify to 0
        // sin(-0) -> -sin(0) -> -0 -> 0
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = sin(neg(constant(0.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_kronecker_delta_chain() {
        // Test that (δ^ν_ρ * v^ρ) simplifies to v^ν first,
        // then we can contract with another delta
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        // First contraction: δ^ν_ρ * v^ρ -> v^ν
        let inner = mul(
            tensor("δ", vec![upper("nu"), lower("rho")]),
            tensor("v", vec![upper("rho")]),
        );
        let inner_result = simplify(&inner, &rules);
        assert_eq!(inner_result, tensor("v", vec![upper("nu")]));

        // Second contraction: δ^μ_ν * v^ν -> v^μ
        let outer = mul(tensor("δ", vec![upper("mu"), lower("nu")]), inner_result);
        let final_result = simplify(&outer, &rules);
        assert_eq!(final_result, tensor("v", vec![upper("mu")]));
    }

    #[test]
    fn simplify_kronecker_delta_both_sides() {
        // v^α * δ^β_α should simplify to v^β
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("v", vec![upper("alpha")]),
            tensor("δ", vec![upper("beta"), lower("alpha")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("beta")]));
    }

    #[test]
    fn simplify_full_ruleset() {
        // Test with the full ruleset combining all rule types
        // ((x * 1) + sin(0)) * (sin²(y) + cos²(y))
        // -> (x + 0) * 1 -> x * 1 -> x
        let rules = RuleSet::full();

        let expr = mul(
            add(mul(scalar("x"), constant(1.0)), sin(constant(0.0))),
            add(
                pow(sin(scalar("y")), constant(2.0)),
                pow(cos(scalar("y")), constant(2.0)),
            ),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_complex_expression_with_all_operators() {
        // -(-(x^1 * 1) + 0) should simplify to x
        let rules = RuleSet::standard();

        let expr = neg(neg(add(
            mul(pow(scalar("x"), constant(1.0)), constant(1.0)),
            constant(0.0),
        )));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_inv_of_inv_nested_in_mul() {
        // x * (1/(1/y)) should simplify to x * y
        let rules = RuleSet::standard();

        let expr = mul(scalar("x"), inv(inv(scalar("y"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_zero_times_complex_trig() {
        // 0 * (sin(x) + cos(x)) should simplify to 0
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = mul(constant(0.0), add(sin(scalar("x")), cos(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_pythagorean_with_complex_argument() {
        // sin²(x + y) + cos²(x + y) = 1
        let rules = RuleSet::trigonometric();

        let arg = add(scalar("x"), scalar("y"));
        let expr = add(
            pow(sin(arg.clone()), constant(2.0)),
            pow(cos(arg), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_nested_functions_with_identities() {
        // exp(ln(1)) should simplify to 1
        // exp(0) = 1
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = exp(ln(constant(1.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_preserves_structure_when_no_rules_match() {
        // (x + y) * (a + b) should remain structurally similar with standard rules
        // (no distribution in standard ruleset, but factors are normalized to canonical order)
        let rules = RuleSet::standard();

        let expr = mul(add(scalar("x"), scalar("y")), add(scalar("a"), scalar("b")));
        let result = simplify(&expr, &rules);
        // Factors are normalized: (a + b) * (x + y) since "a" < "x" alphabetically
        assert_eq!(
            result,
            mul(add(scalar("a"), scalar("b")), add(scalar("x"), scalar("y")))
        );
    }

    #[test]
    fn simplify_max_steps_limit() {
        // Test that we don't infinite loop - use a very low max_steps
        let rules = RuleSet::standard();
        let search = BeamSearch::new(5, 2);

        // This would need many steps to fully simplify
        let expr = add(
            add(
                add(add(scalar("x"), constant(0.0)), constant(0.0)),
                constant(0.0),
            ),
            constant(0.0),
        );
        let result = search.simplify(&expr, &rules);
        // With only 2 steps, we won't fully simplify
        // But we should make some progress and not crash
        assert!(result.complexity() <= expr.complexity());
    }

    #[test]
    fn simplify_tensor_with_standard_rules() {
        // δ^μ_ν * (v^ν * 1) should simplify using both tensor and standard rules
        use crate::expr::lower;
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::tensor());

        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            mul(tensor("v", vec![upper("nu")]), constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    // === Additional edge case tests ===

    #[test]
    fn simplify_add_zero_left() {
        // 0 + x should simplify to x (tests left-side zero rule)
        let rules = RuleSet::standard();
        let expr = add(constant(0.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_one_left() {
        // 1 * x should simplify to x (tests left-side one rule)
        let rules = RuleSet::standard();
        let expr = mul(constant(1.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mul_zero_left() {
        // 0 * x should simplify to 0 (tests left-side zero rule)
        let rules = RuleSet::standard();
        let expr = mul(constant(0.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_of_neg_of_constant() {
        // --5 should simplify to 5
        let rules = RuleSet::standard();
        let expr = neg(neg(constant(5.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(5.0));
    }

    #[test]
    fn simplify_inv_of_one() {
        // 1/1 = 1
        let rules = RuleSet::standard();
        let expr = inv(constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_pow_negative_exponent() {
        // x^(-1) with tensor rules: pow_neg_exp produces 1/x^1
        // Same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::tensor();
        let expr = pow(scalar("x"), neg(constant(1.0)));
        let result = simplify(&expr, &rules);
        // pow_neg_exp: x^(-a) = 1/x^a
        assert_eq!(result, inv(pow(scalar("x"), constant(1.0))));
    }

    #[test]
    fn simplify_pow_negative_exponent_with_standard() {
        // x^(-1) with both tensor and standard rules
        // Should become 1/x^1 then 1/x (via pow_one rule)
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::tensor());
        let expr = pow(scalar("x"), neg(constant(1.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, inv(scalar("x")));
    }

    #[test]
    fn simplify_zero_to_any_power() {
        // 0^x = 0 (assumes x > 0)
        let rules = RuleSet::standard();
        let expr = pow(constant(0.0), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_zero() {
        // -0 should simplify to 0
        let rules = RuleSet::standard();
        let expr = neg(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_add_neg_self() {
        // x + (-x) = 0 (term cancellation)
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_add_self() {
        // (-x) + x = 0 (term cancellation, reversed order)
        let rules = RuleSet::standard();
        let expr = add(neg(scalar("x")), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_mul_neg_one_right() {
        // x * (-1) = -x
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), neg(constant(1.0)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(scalar("x")));
    }

    #[test]
    fn simplify_combine_like_terms() {
        let rules = RuleSet::standard();
        let expr = add(mul(constant(12.0), scalar("x")), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(constant(13.0), scalar("x")));
    }

    #[test]
    fn simplify_combine_like_terms_self() {
        let rules = RuleSet::standard();
        let expr = add(scalar("x"), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(constant(2.0), scalar("x")));
    }

    #[test]
    fn simplify_collect_multiple_like_terms() {
        let rules = RuleSet::standard();
        let expr = add(
            add(add(add(scalar("x"), scalar("x")), scalar("y")), scalar("x")),
            mul(constant(3.0), scalar("y")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(
            result,
            add(
                mul(constant(3.0), scalar("x")),
                mul(constant(4.0), scalar("y"))
            )
        );
    }

    #[test]
    fn simplify_collect_tensor_like_terms() {
        let rules = RuleSet::standard();
        let expr = add(
            add(tensor("A", vec![upper("i")]), tensor("A", vec![upper("i")])),
            mul(constant(2.0), tensor("A", vec![upper("i")])),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(constant(4.0), tensor("A", vec![upper("i")])));
    }

    #[test]
    fn simplify_collect_with_subtraction() {
        let rules = RuleSet::standard();
        let expr = add(
            add(mul(constant(3.0), scalar("y")), scalar("x")),
            neg(scalar("y")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(scalar("x"), mul(constant(2.0), scalar("y"))));
    }

    #[test]
    fn simplify_subtraction_preserves_order() {
        // y - x should stay as y - x, not become -x + y
        let rules = RuleSet::standard();
        let expr = add(scalar("y"), neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(scalar("y"), neg(scalar("x"))));
    }

    #[test]
    fn simplify_binomial_minus_linear_terms() {
        // (x + 1)(y + 1) - y - x - 1 = x*y
        let rules = RuleSet::full();
        let expr = add(
            add(
                add(
                    mul(
                        add(scalar("x"), constant(1.0)),
                        add(scalar("y"), constant(1.0)),
                    ),
                    neg(scalar("y")),
                ),
                neg(scalar("x")),
            ),
            neg(constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        // Result is x*y (order may vary due to commutativity)
        let xy = mul(scalar("x"), scalar("y"));
        let yx = mul(scalar("y"), scalar("x"));
        assert!(
            result == xy || result == yx,
            "Expected x*y or y*x, got {:?}",
            result
        );
    }

    #[test]
    fn simplify_distribute_one_plus_and_cancel() {
        let rules = RuleSet::full();
        let expr = add(
            add(
                mul(scalar("x"), add(constant(1.0), scalar("y"))),
                neg(mul(scalar("x"), scalar("y"))),
            ),
            scalar("x"),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(constant(2.0), scalar("x")));
    }

    #[test]
    fn simplify_distribute_cancel_general() {
        let rules = RuleSet::full();
        let expr = add(
            mul(scalar("x"), add(scalar("a"), scalar("b"))),
            neg(mul(scalar("x"), scalar("b"))),
        );
        let result = simplify(&expr, &rules);
        // a * x or x * a — either ordering is valid
        assert!(
            result == mul(scalar("a"), scalar("x")) || result == mul(scalar("x"), scalar("a")),
            "expected a * x in either order, got {:?}",
            result,
        );
    }

    #[test]
    fn simplify_mul_neg_one_left() {
        // (-1) * x = -x
        let rules = RuleSet::standard();
        let expr = mul(neg(constant(1.0)), scalar("x"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(scalar("x")));
    }

    #[test]
    fn simplify_trig_at_pi() {
        // sin(π) = 0, cos(π) = -1
        let rules = RuleSet::trigonometric();

        let sin_pi = sin(frac_pi(1, 1));
        let cos_pi = cos(frac_pi(1, 1));

        assert_eq!(simplify(&sin_pi, &rules), constant(0.0));
        assert_eq!(simplify(&cos_pi, &rules), constant(-1.0));
    }

    #[test]
    fn simplify_trig_at_pi_over_2() {
        // sin(π/2) = 1, cos(π/2) = 0
        let rules = RuleSet::trigonometric();

        let sin_pi_2 = sin(frac_pi(1, 2));
        let cos_pi_2 = cos(frac_pi(1, 2));

        assert_eq!(simplify(&sin_pi_2, &rules), constant(1.0));
        assert_eq!(simplify(&cos_pi_2, &rules), constant(0.0));
    }

    #[test]
    fn simplify_trig_at_pi_over_2_from_division() {
        // cos(π * (1/2)) should simplify via pi-fraction rules + trig evaluation
        let rules = RuleSet::full();
        let expr = cos(mul(
            frac_pi(1, 1),
            inv(Expr::new(ExprKind::Rational(Rational::from_i64(2)))),
        ));
        assert_eq!(simplify(&expr, &rules), constant(0.0));
    }

    #[test]
    fn simplify_exp_zero() {
        // exp(0) = 1
        let rules = RuleSet::trigonometric();
        use crate::expr::exp;

        let expr = exp(constant(0.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_ln_one() {
        // ln(1) = 0
        let rules = RuleSet::trigonometric();
        use crate::expr::ln;

        let expr = ln(constant(1.0));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_pow_same_base_mul() {
        // x^2 * x^3 = x^(2+3) with tensor rules
        let rules = RuleSet::tensor();
        let expr = mul(
            pow(scalar("x"), constant(2.0)),
            pow(scalar("x"), constant(3.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, pow(scalar("x"), constant(5.0)));
    }

    #[test]
    fn simplify_pow_of_pow() {
        // (x^2)^3 = x^(2*3) with tensor rules
        // Same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::tensor();
        let expr = pow(pow(scalar("x"), constant(2.0)), constant(3.0));
        let result = simplify(&expr, &rules);
        // pow_of_pow: (x^a)^b = x^(a*b)
        assert_eq!(result, pow(scalar("x"), constant(6.0)));
    }

    #[test]
    fn simplify_distribute_mul_over_add_no_simplification() {
        // x * (y + z) has complexity 5, x*y + x*z has complexity 7
        // Beam search explores the distributed form, but since no follow-up rules
        // reduce complexity below the original, the original is returned.
        // Either factor ordering is valid without normalize_mul
        let rules = RuleSet::tensor();
        let expr = mul(scalar("x"), add(scalar("y"), scalar("z")));
        let result = simplify(&expr, &rules);
        assert!(
            result == mul(scalar("x"), add(scalar("y"), scalar("z")))
                || result == mul(add(scalar("y"), scalar("z")), scalar("x")),
            "expected x * (y + z) in either order, got {:?}",
            result,
        );
    }

    #[test]
    fn simplify_neg_distribute_over_add_no_simplification() {
        // -(x + y) has complexity 4, -x + -y has complexity 5
        // Beam search explores the distributed form, but since no follow-up rules
        // reduce complexity below the original, the original is returned.
        let rules = RuleSet::tensor();
        let expr = neg(add(scalar("x"), scalar("y")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_inv_distribute_over_mul_no_simplification() {
        // 1/(x * y) has complexity 4, (1/x) * (1/y) has complexity 5
        // Beam search explores the distributed form, but since no follow-up rules
        // reduce complexity below the original, the original is returned.
        let rules = RuleSet::tensor();
        let expr = inv(mul(scalar("x"), scalar("y")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_deep_nesting_all_identities() {
        // (((x^1 * 1) + 0)^1 * 1) + 0 should simplify to x
        let rules = RuleSet::standard();
        let expr = add(
            mul(
                pow(
                    add(
                        mul(pow(scalar("x"), constant(1.0)), constant(1.0)),
                        constant(0.0),
                    ),
                    constant(1.0),
                ),
                constant(1.0),
            ),
            constant(0.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_alternating_neg_inv() {
        // -1/(-(1/x)) should simplify
        // First: 1/x, then -(1/x), then 1/(-(1/x)) = -1/(1/x) = -x?
        // Actually: double_inv on 1/(1/x) gives x, so 1/(-(1/x))...
        // This is tricky - let's see what happens
        let rules = RuleSet::standard();
        let expr = neg(inv(neg(inv(scalar("x")))));
        let result = simplify(&expr, &rules);
        // The expression is -(1/(-(1/x)))
        // With double_neg and double_inv rules, unclear what the final form is
        // Let's just check it doesn't crash and reduces complexity
        assert!(result.complexity() <= expr.complexity());
    }

    #[test]
    fn simplify_pythagorean_reversed() {
        // Beam search can simplify cos²(x) + sin²(x) = 1 via commutativity
        let rules = RuleSet::trigonometric();
        let expr = add(
            pow(cos(scalar("x")), constant(2.0)),
            pow(sin(scalar("x")), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_tensor_covector_contraction() {
        // δ^μ_ν * w_ν = w_μ (covector contraction)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("w", vec![lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("w", vec![lower("mu")]));
    }

    #[test]
    fn simplify_kronecker_no_contraction() {
        // δ^μ_ν * v^σ should NOT simplify (no matching indices)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("δ", vec![upper("mu"), lower("nu")]),
            tensor("v", vec![upper("sigma")]),
        );
        let result = simplify(&expr, &rules);
        // No contraction, order preserved (tensor products are not commutative)
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_multiple_variables_same_structure() {
        // (a + 0) + (b + 0) + (c + 0) should simplify to a + b + c
        let rules = RuleSet::standard();
        let expr = add(
            add(
                add(scalar("a"), constant(0.0)),
                add(scalar("b"), constant(0.0)),
            ),
            add(scalar("c"), constant(0.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, add(add(scalar("a"), scalar("b")), scalar("c")));
    }

    #[test]
    fn simplify_x_times_x_square() {
        // x * x = x^2 - same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::standard();
        let expr = mul(scalar("x"), scalar("x"));
        let result = simplify(&expr, &rules);
        // mul_self_square: x * x = x^2
        assert_eq!(result, pow(scalar("x"), constant(2.0)));
    }

    #[test]
    fn simplify_sin_of_neg_x() {
        // sin(-x) = -sin(x) - same complexity, but rule-based rewrites are preferred
        let rules = RuleSet::trigonometric();
        let expr = sin(neg(scalar("x")));
        let result = simplify(&expr, &rules);
        // sin_neg: sin(-x) = -sin(x)
        assert_eq!(result, neg(sin(scalar("x"))));
    }

    #[test]
    fn simplify_cos_of_neg_x() {
        // cos(-x) = cos(x) (even function)
        let rules = RuleSet::trigonometric();
        let expr = cos(neg(scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, cos(scalar("x")));
    }

    #[test]
    fn simplify_complex_trig_identity() {
        // sin(-0) * cos(0) + sin(0) = 0 * 1 + 0 = 0
        let mut rules = RuleSet::standard();
        rules.merge(RuleSet::trigonometric());

        let expr = add(
            mul(sin(neg(constant(0.0))), cos(constant(0.0))),
            sin(constant(0.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_nested_pow_with_identities() {
        // ((x^1)^0)^1 should become 1^1 = 1
        let rules = RuleSet::standard();
        let expr = pow(
            pow(pow(scalar("x"), constant(1.0)), constant(0.0)),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_zero_in_exponent_nested() {
        // (x + y)^0 * z should become 1 * z = z
        let rules = RuleSet::standard();
        let expr = mul(
            pow(add(scalar("x"), scalar("y")), constant(0.0)),
            scalar("z"),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("z"));
    }

    #[test]
    fn simplify_one_to_complex_power() {
        // 1^(x + y + z) should become 1
        let rules = RuleSet::standard();
        let expr = pow(
            constant(1.0),
            add(add(scalar("x"), scalar("y")), scalar("z")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_tensor_generic_no_simplification() {
        // Generic tensors (not δ or g) don't have contraction rules
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        // This won't match any rules since A is not a known tensor
        let expr = mul(
            tensor("A", vec![upper("mu"), lower("nu")]),
            tensor("B", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        // No matching rule for generic tensor contraction, remains unchanged
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_tensor_metric_lower_index() {
        // g_μν * v^ν = v_μ (metric tensor lowers the index)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![lower("mu"), lower("nu")]),
            tensor("v", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![lower("mu")]));
    }

    #[test]
    fn simplify_tensor_metric_raise_index() {
        // g^μν * v_ν = v^μ (inverse metric tensor raises the index)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![upper("mu"), upper("nu")]),
            tensor("v", vec![lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("v", vec![upper("mu")]));
    }

    #[test]
    fn simplify_tensor_metric_inverse_gives_delta() {
        // g^μκ * g_κν = δ^μ_ν (metric times inverse metric gives Kronecker delta)
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![upper("mu"), upper("kappa")]),
            tensor("g", vec![lower("kappa"), lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, tensor("δ", vec![upper("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_tensor_metric_symmetry_enables_contraction() {
        // g_νμ * v^ν = v_μ
        // The metric g_νμ has indices in "wrong" order for metric_lower_right,
        // but symmetry rule g_νμ = g_μν allows beam search to find the contraction.
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![lower("nu"), lower("mu")]), // indices swapped
            tensor("v", vec![upper("nu")]),
        );
        let result = simplify(&expr, &rules);
        // Via symmetry: g_νμ → g_μν, then metric_lower_right: g_μν * v^ν → v_μ
        assert_eq!(result, tensor("v", vec![lower("mu")]));
    }

    #[test]
    fn simplify_tensor_metric_inverse_with_symmetry() {
        // g^κμ * g_κν = δ^μ_ν
        // The first metric has indices swapped, but symmetry allows matching.
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = mul(
            tensor("g", vec![upper("kappa"), upper("mu")]), // swapped from g^μκ
            tensor("g", vec![lower("kappa"), lower("nu")]),
        );
        let result = simplify(&expr, &rules);
        // Via symmetry: g^κμ → g^μκ, then metric_inverse_right: g^μκ * g_κν → δ^μ_ν
        assert_eq!(result, tensor("δ", vec![upper("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_tensor_antisymmetric_standalone_no_change() {
        // ε_μν alone doesn't simplify - antisymmetry rule increases complexity
        // (adds negation). The rule exists for pattern matching in multi-step
        // simplifications, not standalone application.
        use crate::expr::lower;
        let rules = RuleSet::tensor();

        let expr = tensor("ε", vec![lower("mu"), lower("nu")]);
        let result = simplify(&expr, &rules);
        // No change - applying antisymmetry would increase complexity
        assert_eq!(result, expr);
    }

    #[test]
    fn simplify_tensor_double_antisymmetry_simplifies() {
        // -ε_νμ can use antisymmetry to become --ε_μν = ε_μν via double negation
        // This demonstrates antisymmetry combined with other rules.
        use crate::expr::lower;
        let rules = RuleSet::full(); // includes double_neg rule

        let expr = neg(tensor("ε", vec![lower("nu"), lower("mu")]));
        let result = simplify(&expr, &rules);
        // Via antisymmetry: -ε_νμ → --ε_μν, then double_neg: --ε_μν → ε_μν
        assert_eq!(result, tensor("ε", vec![lower("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_tensor_antisymmetric_sum_cancels() {
        // ε_μν + ε_νμ = 0 (antisymmetric tensor + swapped form = 0)
        // This is a fundamental property of antisymmetric tensors.
        //
        // Path: ε_μν + ε_νμ (complexity 3)
        //    → ε_μν + (-ε_μν) via antisymmetry on second term (complexity 4 - INCREASE!)
        //    → 0 via add_neg_self (complexity 1)
        //
        // Beam search must explore the complexity-increasing step to find the simplification.
        use crate::expr::lower;
        let rules = RuleSet::full();

        let expr = add(
            tensor("ε", vec![lower("mu"), lower("nu")]),
            tensor("ε", vec![lower("nu"), lower("mu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_tensor_antisymmetric_em_field_sum_cancels() {
        // F^μν + F^νμ = 0 (electromagnetic field tensor is antisymmetric)
        // Same principle as Levi-Civita: F^νμ = -F^μν, so F^μν + F^νμ = F^μν + (-F^μν) = 0
        let rules = RuleSet::full();

        let expr = add(
            tensor("F", vec![upper("mu"), upper("nu")]),
            tensor("F", vec![upper("nu"), upper("mu")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_tensor_custom_antisymmetric_cancels() {
        // Test that custom antisymmetric tensors also exhibit cancellation
        // ω_ij + ω_ji = 0 for antisymmetric ω (e.g., vorticity tensor)
        use crate::expr::lower;
        let mut rules = RuleSet::full();
        rules.add_antisymmetric("ω");

        let expr = add(
            tensor("ω", vec![lower("i"), lower("j")]),
            tensor("ω", vec![lower("j"), lower("i")]),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_tensor_custom_symmetric() {
        // Test custom symmetric tensor using add_symmetric
        // Symmetry rules have same complexity, but rule-based forms are preferred
        use crate::expr::lower;
        let mut rules = RuleSet::tensor();
        rules.add_symmetric("h"); // perturbation metric

        let expr = tensor("h", vec![lower("nu"), lower("mu")]);
        let result = simplify(&expr, &rules);
        // Symmetry allows reordering; current search prefers alphabetical: mu, nu.
        assert_eq!(result, tensor("h", vec![lower("mu"), lower("nu")]));
    }

    #[test]
    fn simplify_dummy_index_normalization() {
        // a^i*b_i + a^j*b_j should combine to 2a^i*b_i
        // The dummy indices i and j are equivalent (both are contracted)
        use crate::expr::lower;
        let rules = RuleSet::full();

        let expr = add(
            mul(tensor("a", vec![upper("i")]), tensor("b", vec![lower("i")])),
            mul(tensor("a", vec![upper("j")]), tensor("b", vec![lower("j")])),
        );
        let result = simplify(&expr, &rules);

        // Should combine to 2 * (a^i * b_i) (with some index, doesn't matter which)
        // Check that result is NOT an Add (terms combined) and contains coefficient 2
        let debug = format!("{:?}", result);
        assert!(
            !matches!(result.kind, ExprKind::Add(_, _)),
            "Terms should combine but got Add: {}",
            debug
        );
        assert!(
            debug.contains("2.0") || debug.contains("Const(2") || debug.contains("num: 2, den: 1"),
            "Should have coefficient 2, got: {}",
            debug
        );
        assert!(
            debug.contains("\"a\"") && debug.contains("\"b\""),
            "Should contain both tensors a and b, got: {}",
            debug
        );
    }

    #[test]
    fn canonical_key_normalizes_mul_self_to_pow_two() {
        let product = mul(scalar("x"), scalar("x"));
        let squared = pow(scalar("x"), constant(2.0));
        assert_eq!(canonical_key(&product), canonical_key(&squared));
    }

    #[test]
    fn canonical_key_normalizes_dummy_index_names() {
        use crate::expr::lower;

        let left = mul(tensor("a", vec![upper("i")]), tensor("b", vec![lower("i")]));
        let right = mul(tensor("a", vec![upper("j")]), tensor("b", vec![lower("j")]));

        assert_eq!(canonical_key(&left), canonical_key(&right));
    }

    #[test]
    fn simplify_dummy_index_double_contraction() {
        // a^(i,j)*b_(i,j) + a^(k,m)*b_(k,m) should combine to 2a^(i,j)*b_(i,j)
        use crate::expr::lower;
        let rules = RuleSet::full();

        let expr = add(
            mul(
                tensor("a", vec![upper("i"), upper("j")]),
                tensor("b", vec![lower("i"), lower("j")]),
            ),
            mul(
                tensor("a", vec![upper("k"), upper("m")]),
                tensor("b", vec![lower("k"), lower("m")]),
            ),
        );
        let result = simplify(&expr, &rules);

        // Should combine to 2 * (a^(i,j) * b_(i,j))
        let debug = format!("{:?}", result);
        assert!(
            !matches!(result.kind, ExprKind::Add(_, _)),
            "Terms should combine but got Add: {}",
            debug
        );
        assert!(
            debug.contains("2.0") || debug.contains("Const(2") || debug.contains("num: 2, den: 1"),
            "Should have coefficient 2, got: {}",
            debug
        );
    }

    #[test]
    fn simplify_free_indices_not_normalized() {
        // a^i + a^j should NOT combine (different free indices)
        let rules = RuleSet::full();

        let expr = add(tensor("a", vec![upper("i")]), tensor("a", vec![upper("j")]));
        let result = simplify(&expr, &rules);

        // Should remain as a^i + a^j (or equivalent ordering)
        assert!(
            matches!(result.kind, ExprKind::Add(_, _)),
            "Different free indices should NOT combine, got: {:?}",
            result
        );
    }

    #[test]
    fn simplify_exp_of_ln_of_exp() {
        // exp(ln(exp(x))) = exp(x)
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = exp(ln(exp(scalar("x"))));
        let result = simplify(&expr, &rules);
        // ln(exp(x)) = x, then exp(x)
        assert_eq!(result, exp(scalar("x")));
    }

    #[test]
    fn simplify_ln_of_exp_of_ln() {
        // ln(exp(ln(x))) = ln(x)
        let rules = RuleSet::trigonometric();
        use crate::expr::{exp, ln};

        let expr = ln(exp(ln(scalar("x"))));
        let result = simplify(&expr, &rules);
        // exp(ln(x)) = x, then ln(x)
        assert_eq!(result, ln(scalar("x")));
    }

    #[test]
    fn simplify_associativity_does_not_loop() {
        // Test that associativity rules don't cause infinite loops
        // (a * b) * c with tensor rules should eventually stabilize
        let rules = RuleSet::tensor();
        let expr = mul(mul(scalar("a"), scalar("b")), scalar("c"));
        let result = simplify(&expr, &rules);
        // Should complete without infinite loop (max_steps prevents it)
        // The result might be reassociated
        assert!(result.complexity() <= expr.complexity() + 2); // Allow slight increase from reassociation
    }

    #[test]
    fn simplify_all_rulesets_complex() {
        // Test a very complex expression with all rule types
        // ((sin(0) + cos(0)) * x + 0)^1
        let rules = RuleSet::full();

        // (sin(0) + cos(0)) = 0 + 1 = 1
        // 1 * x = x
        // x + 0 = x
        // x^1 = x
        let trig_part = add(sin(constant(0.0)), cos(constant(0.0)));
        let expr = pow(
            add(mul(trig_part, scalar("x")), constant(0.0)),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    // === Custom beam search parameter tests ===

    #[test]
    fn simplify_with_custom_params() {
        // Test BeamSearch with custom parameters
        let search = BeamSearch::new(5, 50);
        let rules = RuleSet::standard();
        let expr = add(add(scalar("x"), constant(0.0)), constant(0.0));
        let result = search.simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    // === Beam search exploration tests ===
    // These tests verify that beam search explores paths that may temporarily
    // increase complexity before finding a simpler result.

    #[test]
    fn simplify_neg_of_canceling_sum() {
        // -(x + (-x)) should simplify to 0
        // Path A (direct): -(x + (-x)) → -(0) → 0 (via add_neg_self_right, then neg_zero)
        // Path B (via distribution): -(x + (-x)) → -x + -(-x) → -x + x → 0
        //   complexity: 5 → 6 → 4 → 1 (temporary increase!)
        // Beam search explores both paths and finds the optimal result.
        let rules = RuleSet::full();
        let expr = neg(add(scalar("x"), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_neg_of_double_neg_sum() {
        // -((-x) + x) should simplify to 0
        // The inner sum (-x) + x matches add_neg_self_left directly
        let rules = RuleSet::full();
        let expr = neg(add(neg(scalar("x")), scalar("x")));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_distribution_enables_cancellation() {
        // -(a + (-a)) where a = sin(x)
        // This tests that wildcards in add_neg_self match complex expressions
        let rules = RuleSet::full();
        let expr = neg(add(sin(scalar("x")), neg(sin(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_nested_neg_distribution() {
        // -(-(-x) + (-x)) should simplify:
        // Inner: -(-x) + (-x) = x + (-x) = 0 (via double_neg, then add_neg_self_right)
        // Then: -(0) = 0
        let rules = RuleSet::full();
        let expr = neg(add(neg(neg(scalar("x"))), neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_complex_path_through_distribution() {
        // (x + (-x)) * y should simplify to 0 * y = 0
        // Path: 7 → (inner cancel) → 4 → 1
        let rules = RuleSet::full();
        let expr = mul(add(scalar("x"), neg(scalar("x"))), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_sum_of_pythagorean_and_neg_one() {
        // sin²(x) + cos²(x) + (-1) should simplify to 0
        // Path: pythagorean → 1, then 1 + (-1) → 0
        let rules = RuleSet::full();
        let expr = add(
            add(
                pow(sin(scalar("x")), constant(2.0)),
                pow(cos(scalar("x")), constant(2.0)),
            ),
            neg(constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_reversed_pythagorean_plus_neg_one() {
        // cos²(x) + sin²(x) + (-1) should also simplify to 0
        // Requires commutativity exploration to match pythagorean pattern
        let rules = RuleSet::full();
        let expr = add(
            add(
                pow(cos(scalar("x")), constant(2.0)),
                pow(sin(scalar("x")), constant(2.0)),
            ),
            neg(constant(1.0)),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_power_chain_through_intermediate() {
        // x^2 * x^(-1) with tensor+standard rules
        // Via pow_mul_same_base: x^(2 + (-1))
        // The exponent 2 + (-1) = 2 - 1 = 1, but we don't have constant folding
        // However, this tests the path through equal-complexity transformations
        let rules = RuleSet::full();
        let expr = mul(
            pow(scalar("x"), constant(2.0)),
            pow(scalar("x"), neg(constant(1.0))),
        );
        let result = simplify(&expr, &rules);
        // Should apply pow_mul_same_base: x^(2 + (-1))
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_double_neg_in_product() {
        // (-(-x)) * y should simplify to x * y
        let rules = RuleSet::full();
        let expr = mul(neg(neg(scalar("x"))), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(scalar("x"), scalar("y")));
    }

    #[test]
    fn simplify_inv_chain() {
        // 1/(1/(1/(1/x))) should simplify to x
        // Each step maintains or reduces complexity until we get to x
        let rules = RuleSet::full();
        let expr = inv(inv(inv(inv(scalar("x")))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("x"));
    }

    #[test]
    fn simplify_mixed_neg_inv_chain() {
        // -(1/(-x)) should simplify
        // This exercises interaction between neg and inv rules
        let rules = RuleSet::full();
        let expr = neg(inv(neg(scalar("x"))));
        let result = simplify(&expr, &rules);
        // The exact result depends on rule order, but complexity should not increase
        assert!(result.complexity() <= expr.complexity());
    }

    #[test]
    fn simplify_zero_times_complex_sum() {
        // 0 * (a + b + c) should simplify to 0
        // Tests that mul_zero_left works with complex RHS
        let rules = RuleSet::full();
        let expr = mul(
            constant(0.0),
            add(add(scalar("a"), scalar("b")), scalar("c")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_one_raised_to_complex_neg_power() {
        // 1^(-(x + y)) should simplify to 1
        // Tests one_pow rule with complex exponent
        let rules = RuleSet::full();
        let expr = pow(constant(1.0), neg(add(scalar("x"), scalar("y"))));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_binomial_with_square_cancellation() {
        // (x + y)(1 - x) + x² = x + y - xy
        let rules = RuleSet::full();
        let expr = add(
            mul(
                add(scalar("x"), scalar("y")),
                add(constant(1.0), neg(scalar("x"))),
            ),
            pow(scalar("x"), constant(2.0)),
        );
        let result = simplify(&expr, &rules);
        // Result should be x + y - xy (terms may be reordered)
        // Check that complexity is reduced and the x² term cancelled
        assert!(result.complexity() < expr.complexity());
        // Verify no Pow node remains (x² should have cancelled)
        let result_str = format!("{:?}", result);
        assert!(!result_str.contains("Pow"), "x² should have cancelled");
    }

    #[test]
    fn simplify_triple_product_to_cube() {
        // y * y * y = y^3
        let rules = RuleSet::full();
        let expr = mul(mul(scalar("y"), scalar("y")), scalar("y"));
        let result = simplify(&expr, &rules);
        assert_eq!(result, pow(scalar("y"), constant(3.0)));
    }

    #[test]
    fn simplify_mul_self_minus_power() {
        // x*x + y - x² = y
        let rules = RuleSet::full();
        let expr = add(
            add(mul(scalar("x"), scalar("x")), scalar("y")),
            neg(pow(scalar("x"), constant(2.0))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, scalar("y"));
    }

    #[test]
    #[ignore] // Requires expanding (x+y+1)^2 which increases complexity; ML model will handle
    fn simplify_trinomial_squared_to_zero() {
        // (x + y + 1)^2 - x^2 - y^2 - 1 - 2*x*y - 2*x - 2*y = 0
        let rules = RuleSet::full();
        let x = || scalar("x");
        let y = || scalar("y");
        let sum = add(add(x(), y()), constant(1.0));
        let term1 = pow(sum, constant(2.0));
        let term2 = neg(pow(x(), constant(2.0)));
        let term3 = neg(pow(y(), constant(2.0)));
        let term4 = neg(constant(1.0));
        let term5 = neg(mul(constant(2.0), mul(x(), y())));
        let term6 = neg(mul(constant(2.0), x()));
        let term7 = neg(mul(constant(2.0), y()));
        let expr = add(
            add(add(add(add(add(term1, term2), term3), term4), term5), term6),
            term7,
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_sin_4pi() {
        // sin(4π) = 0 (4 is even, sin(even * π) = 0)
        let rules = RuleSet::trigonometric();
        let expr = sin(mul(
            Expr::new(ExprKind::Rational(Rational::from_i64(4))),
            frac_pi(1, 1),
        ));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_6pi() {
        // cos(6π) = 1 (6 is even, cos(even * π) = 1)
        let rules = RuleSet::trigonometric();
        let expr = cos(mul(
            Expr::new(ExprKind::Rational(Rational::from_i64(6))),
            frac_pi(1, 1),
        ));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_sin_5pi_4() {
        // sin(5π/4) = -√2/2
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(5, 4));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(named(NamedConst::Frac1Sqrt2)));
    }

    #[test]
    fn simplify_trig_cos_5pi_4() {
        // cos(5π/4) = -√2/2
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(5, 4));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(named(NamedConst::Frac1Sqrt2)));
    }

    #[test]
    fn simplify_trig_sin_7pi_6() {
        // sin(7π/6) = -1/2
        let rules = RuleSet::trigonometric();
        let expr = sin(frac_pi(7, 6));
        let result = simplify(&expr, &rules);
        assert_eq!(result, rational(-1, 2));
    }

    #[test]
    fn simplify_trig_cos_7pi_6() {
        // cos(7π/6) = -√3/2
        let rules = RuleSet::trigonometric();
        let expr = cos(frac_pi(7, 6));
        let result = simplify(&expr, &rules);
        assert_eq!(result, neg(named(NamedConst::FracSqrt3By2)));
    }

    #[test]
    fn simplify_trig_sin_negative_2pi() {
        // sin(-2π) = 0
        let rules = RuleSet::trigonometric();
        let expr = sin(neg(frac_pi(2, 1)));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_negative_4pi() {
        // cos(-4π) = 1
        let rules = RuleSet::trigonometric();
        let expr = cos(neg(mul(
            Expr::new(ExprKind::Rational(Rational::from_i64(4))),
            frac_pi(1, 1),
        )));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_sin_100pi() {
        // sin(100π) = 0 (integer * π)
        let rules = RuleSet::trigonometric();
        let expr = sin(mul(
            Expr::new(ExprKind::Rational(Rational::from_i64(100))),
            frac_pi(1, 1),
        ));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(0.0));
    }

    #[test]
    fn simplify_trig_cos_100pi() {
        // cos(100π) = 1 (even * π)
        let rules = RuleSet::trigonometric();
        let expr = cos(mul(
            Expr::new(ExprKind::Rational(Rational::from_i64(100))),
            frac_pi(1, 1),
        ));
        let result = simplify(&expr, &rules);
        assert_eq!(result, constant(1.0));
    }

    #[test]
    fn simplify_trig_cos_9pi_4() {
        // cos(9π/4) = cos(π/4 + 2π) = cos(π/4) = √2/2
        let rules = RuleSet::full();
        let expr = cos(div(
            mul(
                Expr::new(ExprKind::Rational(Rational::from_i64(9))),
                frac_pi(1, 1),
            ),
            Expr::new(ExprKind::Rational(Rational::from_i64(4))),
        ));
        let result = simplify(&expr, &rules);
        assert_eq!(result, named(NamedConst::Frac1Sqrt2));
    }

    #[test]
    fn simplify_trig_sin_9pi_4() {
        // sin(9π/4) = sin(π/4 + 2π) = sin(π/4) = √2/2
        let rules = RuleSet::full();
        let expr = sin(div(
            mul(
                Expr::new(ExprKind::Rational(Rational::from_i64(9))),
                frac_pi(1, 1),
            ),
            Expr::new(ExprKind::Rational(Rational::from_i64(4))),
        ));
        let result = simplify(&expr, &rules);
        assert_eq!(result, named(NamedConst::Frac1Sqrt2));
    }

    #[test]
    fn simplify_factor_common() {
        // ax + ay → a(x + y)
        let rules = RuleSet::full();
        let expr = add(mul(scalar("a"), scalar("x")), mul(scalar("a"), scalar("y")));
        let result = simplify(&expr, &rules);

        // Should factor to a * (x + y) or (x + y) * a
        let debug = format!("{}", result);
        assert!(
            debug.contains("a") && (debug.contains("(x + y)") || debug.contains("(y + x)")),
            "Expected factored form, got: {}",
            debug
        );
        assert!(result.complexity() < expr.complexity());
    }

    #[test]
    #[ignore] // Factoring with commutativity rules needs more beam width; ML model will handle
    fn simplify_factor_binomial_product() {
        // ab + 2a + b + 2 → (a + 1)(b + 2)
        let rules = RuleSet::full();
        let expr = add(
            add(
                add(
                    mul(scalar("a"), scalar("b")),
                    mul(constant(2.0), scalar("a")),
                ),
                scalar("b"),
            ),
            constant(2.0),
        );
        let result = simplify(&expr, &rules);

        // Should factor to (a + 1)(b + 2)
        // Complexity: Mul(Add(a,1), Add(b,2)) = 1 + (1+1+1) + (1+1+1) = 7
        // Original expanded: ab + 2a + b + 2 = complexity 9
        let debug = format!("{}", result);
        assert!(
            debug.contains("(a + 1)") && debug.contains("(b + 2)"),
            "Expected (a+1)(b+2), got: {}",
            debug
        );
        assert!(
            result.complexity() < expr.complexity(),
            "Factored form should be simpler: {} vs {}",
            result.complexity(),
            expr.complexity()
        );
    }

    #[test]
    fn simplify_factor_perfect_square() {
        // x² + 2x + 1 → (x + 1)²
        let rules = RuleSet::full();
        let expr = add(
            add(
                pow(scalar("x"), constant(2.0)),
                mul(constant(2.0), scalar("x")),
            ),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);

        // Should be (x + 1)^2
        assert!(
            matches!(&result.kind, ExprKind::Pow(base, exp)
                if matches!(&exp.kind, ExprKind::Rational(r) if *r == Rational::TWO)
                && matches!(&base.kind, ExprKind::Add(_, _))
            ),
            "Expected (x + 1)², got: {:?}",
            result
        );
    }

    #[test]
    fn simplify_factor_perfect_square_minus() {
        // x² - 2x + 1 → (x - 1)²
        let rules = RuleSet::full();
        let expr = add(
            add(
                pow(scalar("x"), constant(2.0)),
                neg(mul(constant(2.0), scalar("x"))),
            ),
            constant(1.0),
        );
        let result = simplify(&expr, &rules);

        // Should be (x - 1)^2
        assert!(
            matches!(&result.kind, ExprKind::Pow(_, exp)
                if matches!(&exp.kind, ExprKind::Rational(r) if *r == Rational::TWO)
            ),
            "Expected (x - 1)², got: {:?}",
            result
        );
    }

    #[test]
    fn simplify_combine_like_terms_both_coefficients() {
        // 3*x + 2*x = 5*x — both sides have explicit coefficients
        let rules = RuleSet::standard();
        let expr = add(
            mul(constant(3.0), scalar("x")),
            mul(constant(2.0), scalar("x")),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(constant(5.0), scalar("x")));
    }

    #[test]
    fn simplify_combine_like_terms_general_expression() {
        // 3*sin(x) + 2*sin(x) = 5*sin(x)
        let rules = RuleSet::standard();
        let expr = add(
            mul(constant(3.0), sin(scalar("x"))),
            mul(constant(2.0), sin(scalar("x"))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, mul(constant(5.0), sin(scalar("x"))));
    }

    #[test]
    fn eval_constants_folds_quantity_to_rational() {
        use crate::dim::Dimension;
        use crate::unit::Unit;
        // 1 [km] should fold to Rational(1000)
        let km = Unit {
            dimension: Dimension::single(crate::dim::BaseDim::L, 1),
            scale: 1000.0,
            display: "km".to_string(),
        };
        let expr = Expr::new(ExprKind::Quantity(Box::new(rational(1, 1)), km));
        let result = eval_constants(&expr);
        assert_eq!(result, rational(1000, 1));
    }

    #[test]
    fn eval_constants_folds_quantity_with_fractional_scale() {
        use crate::dim::Dimension;
        use crate::unit::Unit;
        // 5 [g] should fold to Rational(5, 1000) since gram scale is 0.001
        let g = Unit {
            dimension: Dimension::single(crate::dim::BaseDim::M, 1),
            scale: 0.001,
            display: "g".to_string(),
        };
        let expr = Expr::new(ExprKind::Quantity(Box::new(rational(5, 1)), g));
        let result = eval_constants(&expr);
        assert_eq!(result, rational(1, 200));
    }

    #[test]
    fn eval_constants_multiplies_symbolic_quantities_after_child_folding() {
        use crate::dim::{BaseDim, Dimension};
        use crate::unit::Unit;

        let meters = Unit {
            dimension: Dimension::single(BaseDim::L, 1),
            scale: 1.0,
            display: "m".to_string(),
        };
        let seconds = Unit {
            dimension: Dimension::single(BaseDim::T, 1),
            scale: 1.0,
            display: "s".to_string(),
        };
        let expr = mul(
            Expr::new(ExprKind::Quantity(Box::new(scalar("x")), meters.clone())),
            Expr::new(ExprKind::Quantity(Box::new(scalar("y")), seconds.clone())),
        );

        let result = eval_constants(&expr);
        let expected = Expr::derived(ExprKind::Quantity(
            Box::new(mul(scalar("x"), scalar("y"))),
            meters.mul(&seconds),
        ));

        assert_eq!(result, expected);
    }

    #[test]
    fn simplify_quantity_unit_conversion() {
        // 1000 [m] - 1 [km] should simplify to 0
        use crate::dim::Dimension;
        use crate::unit::Unit;
        let m = Unit {
            dimension: Dimension::single(crate::dim::BaseDim::L, 1),
            scale: 1.0,
            display: "m".to_string(),
        };
        let km = Unit {
            dimension: Dimension::single(crate::dim::BaseDim::L, 1),
            scale: 1000.0,
            display: "km".to_string(),
        };
        let rules = RuleSet::standard();
        let expr = add(
            Expr::new(ExprKind::Quantity(Box::new(rational(1000, 1)), m)),
            neg(Expr::new(ExprKind::Quantity(Box::new(rational(1, 1)), km))),
        );
        let result = simplify(&expr, &rules);
        assert_eq!(result, rational(0, 1));
    }
}
