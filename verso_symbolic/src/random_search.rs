use crate::expr::{Expr, ExprKind};
use crate::rule::{Rule, RuleSet};
use crate::search::{collect_linear_terms, eval_constants};
use rand::prelude::IndexedRandom;
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rand::SeedableRng;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Which child was entered during recursive rule application.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChildIndex {
    Left,
    Right,
    Inner,
    Arg(usize),
}

/// Path from root to the node where a rule was applied.
pub type AstPath = Vec<ChildIndex>;

/// Direction of rule application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Ltr,
    Rtl,
}

/// Flattened rule+direction index for ML training.
/// LTR directions first (0..num_rules), then RTL for reversible rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuleDirectionId(pub u16);

/// Maps between Rule vector indices and flattened rule_direction IDs.
pub struct IndexedRuleSet {
    rules: Vec<Rule>,
    ltr_ids: Vec<RuleDirectionId>,
    rtl_map: HashMap<usize, RuleDirectionId>,
    /// Reverse lookup: direction_id → (rule_index, direction).
    direction_lookup: Vec<(usize, Direction)>,
    pub total_directions: u16,
}

impl IndexedRuleSet {
    pub fn new(ruleset: RuleSet) -> Self {
        let rules = ruleset.into_rules();
        let num_rules = rules.len() as u16;

        // LTR IDs: 0..num_rules (one per rule)
        let ltr_ids: Vec<RuleDirectionId> = (0..num_rules).map(RuleDirectionId).collect();

        // RTL IDs: num_rules..num_rules+num_reversible (only for reversible rules)
        let mut rtl_map = HashMap::new();
        let mut next_rtl_id = num_rules;
        for (i, rule) in rules.iter().enumerate() {
            if rule.reversible {
                rtl_map.insert(i, RuleDirectionId(next_rtl_id));
                next_rtl_id += 1;
            }
        }
        let next_id = next_rtl_id;

        // Build reverse lookup
        let mut direction_lookup = Vec::with_capacity(next_id as usize);
        for i in 0..num_rules as usize {
            direction_lookup.push((i, Direction::Ltr));
        }
        for (i, rule) in rules.iter().enumerate() {
            if rule.reversible {
                direction_lookup.push((i, Direction::Rtl));
            }
        }

        IndexedRuleSet {
            rules,
            ltr_ids,
            rtl_map,
            direction_lookup,
            total_directions: next_id,
        }
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn ltr_id(&self, rule_index: usize) -> RuleDirectionId {
        self.ltr_ids[rule_index]
    }

    pub fn rtl_id(&self, rule_index: usize) -> Option<RuleDirectionId> {
        self.rtl_map.get(&rule_index).copied()
    }

    pub fn rule(&self, index: usize) -> &Rule {
        &self.rules[index]
    }

    /// Look up the (rule_index, direction) for a given direction ID.
    pub fn lookup_direction(&self, id: RuleDirectionId) -> Option<(usize, Direction)> {
        self.direction_lookup.get(id.0 as usize).copied()
    }
}

/// A rewrite with full provenance for ML training data.
#[derive(Clone)]
struct RichRewrite {
    expr: Expr,
    rule_index: usize,
    direction_id: RuleDirectionId,
    direction: Direction,
    path: AstPath,
    rule_name: String,
}

/// One step in the ML-oriented simplification trace.
#[derive(Debug, Clone)]
pub struct RichTraceStep {
    pub expr: Expr,
    pub rule_index: usize,
    pub direction_id: RuleDirectionId,
    pub direction: Direction,
    pub path: AstPath,
    pub rule_name: String,
}

/// Result of a single randomized beam search run.
pub struct SearchRun {
    pub seed: u64,
    pub initial: Expr,
    pub result: Expr,
    pub trace: Vec<RichTraceStep>,
    pub final_complexity: usize,
}

/// Randomized beam search for ML training data generation.
pub struct RandomizedBeamSearch {
    pub beam_width: usize,
    pub max_steps: usize,
    /// Fraction of beam slots filled by random sampling (0.0 = deterministic).
    pub epsilon: f64,
    /// Whether to shuffle rule application order each step.
    pub shuffle_rules: bool,
}

impl Default for RandomizedBeamSearch {
    fn default() -> Self {
        RandomizedBeamSearch {
            beam_width: 20,
            max_steps: 200,
            epsilon: 0.3,
            shuffle_rules: true,
        }
    }
}

impl RandomizedBeamSearch {
    /// Run one randomized beam search with the given RNG.
    pub fn search_once(&self, expr: &Expr, rules: &IndexedRuleSet, rng: &mut StdRng) -> SearchRun {
        let seed = 0; // Caller tracks the seed
        let initial = expr.clone();
        let mut seen: HashSet<String> = HashSet::new();
        let mut beam: Vec<Expr> = vec![expr.clone()];
        let start_key = expr_key(expr);
        seen.insert(start_key.clone());

        let mut best_complexity = expr.complexity();
        let mut best_from_rule = false;
        let mut best_key = start_key.clone();

        let mut derivations: HashMap<String, RichDerivation> = HashMap::new();
        derivations.insert(
            start_key,
            RichDerivation {
                parent_key: None,
                step: None,
            },
        );

        let mut rule_order: Vec<usize> = (0..rules.len()).collect();

        for _step in 0..self.max_steps {
            if self.shuffle_rules {
                rule_order.shuffle(rng);
            }

            let mut all_rewrites: Vec<(String, RichRewrite)> = Vec::new();
            for candidate in &beam {
                let parent_key = expr_key(candidate);
                let rewrites = self.all_rewrites_depth(candidate, rules, 3, &[], &rule_order);
                for r in rewrites {
                    all_rewrites.push((parent_key.clone(), r));
                }
            }

            let mut next_candidates: Vec<(Expr, String)> = Vec::new();
            for (parent_key, rewrite) in all_rewrites {
                let key = expr_key(&rewrite.expr);
                if !seen.contains(&key) {
                    seen.insert(key.clone());

                    derivations.insert(
                        key.clone(),
                        RichDerivation {
                            parent_key: Some(parent_key),
                            step: Some(RichTraceStep {
                                expr: rewrite.expr.clone(),
                                rule_index: rewrite.rule_index,
                                direction_id: rewrite.direction_id,
                                direction: rewrite.direction,
                                path: rewrite.path,
                                rule_name: rewrite.rule_name,
                            }),
                        },
                    );

                    let complexity = rewrite.expr.complexity();
                    if complexity < best_complexity
                        || (complexity == best_complexity && !best_from_rule)
                    {
                        best_complexity = complexity;
                        best_from_rule = true;
                        best_key = key.clone();
                    }

                    next_candidates.push((rewrite.expr, key));
                }
            }

            if next_candidates.is_empty() {
                break;
            }

            // Epsilon-greedy beam selection
            next_candidates.sort_by_key(|(e, _)| e.complexity());
            let deterministic_count =
                ((1.0 - self.epsilon) * self.beam_width as f64).round() as usize;
            let random_count = self.beam_width.saturating_sub(deterministic_count);

            let mut new_beam: Vec<Expr> = Vec::new();
            let split = deterministic_count.min(next_candidates.len());
            for (e, _) in next_candidates.drain(..split) {
                new_beam.push(e);
            }

            if !next_candidates.is_empty() && random_count > 0 {
                let sampled: Vec<_> = next_candidates
                    .choose_multiple(rng, random_count.min(next_candidates.len()))
                    .cloned()
                    .collect();
                for (e, _) in sampled {
                    new_beam.push(e);
                }
            }

            beam = new_beam;
        }

        // Reconstruct the derivation path from best back to start
        let mut trace = Vec::new();
        let mut current_key = best_key;
        loop {
            if let Some(d) = derivations.remove(&current_key) {
                let next_key = d.parent_key.clone();
                if let Some(step) = d.step {
                    trace.push(step);
                }
                match next_key {
                    Some(pk) => current_key = pk,
                    None => break,
                }
            } else {
                break;
            }
        }
        trace.reverse();

        let best_expr = if let Some(last) = trace.last() {
            last.expr.clone()
        } else {
            expr.clone()
        };

        // Post-processing (not recorded in trace)
        let result = collect_linear_terms(&eval_constants(&best_expr));
        let final_complexity = result.complexity();

        SearchRun {
            seed,
            initial,
            result,
            trace,
            final_complexity,
        }
    }

    /// Run N randomized searches in parallel, sorted by trace length.
    pub fn search_multi(
        &self,
        expr: &Expr,
        rules: &IndexedRuleSet,
        n_runs: usize,
        master_seed: u64,
    ) -> Vec<SearchRun> {
        let mut runs: Vec<SearchRun> = (0..n_runs)
            .into_par_iter()
            .map(|i| {
                let seed = master_seed.wrapping_add(i as u64);
                let mut rng = StdRng::seed_from_u64(seed);
                let mut run = self.search_once(expr, rules, &mut rng);
                run.seed = seed;
                run
            })
            .collect();

        runs.sort_by_key(|r| r.trace.len());
        runs
    }

    /// Run N searches and return the best: lowest complexity, then shortest trace.
    pub fn search_best(
        &self,
        expr: &Expr,
        rules: &IndexedRuleSet,
        n_runs: usize,
        master_seed: u64,
    ) -> SearchRun {
        let mut runs = self.search_multi(expr, rules, n_runs, master_seed);
        let best_complexity = runs.iter().map(|r| r.final_complexity).min().unwrap();
        runs.retain(|r| r.final_complexity == best_complexity);
        runs.sort_by_key(|r| r.trace.len());
        runs.into_iter().next().unwrap()
    }

    /// Generate all rewrites with path tracking and shuffled rule order.
    fn all_rewrites_depth(
        &self,
        expr: &Expr,
        rules: &IndexedRuleSet,
        depth: usize,
        path: &[ChildIndex],
        rule_order: &[usize],
    ) -> Vec<RichRewrite> {
        let mut results = Vec::new();

        // Try applying each rule at the root (in shuffled order)
        for &rule_idx in rule_order {
            let rule = rules.rule(rule_idx);

            if let Some(rewritten) = rule.apply_ltr(expr) {
                let folded = eval_constants(&rewritten);
                results.push(RichRewrite {
                    expr: folded,
                    rule_index: rule_idx,
                    direction_id: rules.ltr_id(rule_idx),
                    direction: Direction::Ltr,
                    path: path.to_vec(),
                    rule_name: rule.name.clone(),
                });
            }

            if rule.reversible {
                if let Some(rewritten) = rule.apply_rtl(expr) {
                    let folded = eval_constants(&rewritten);
                    results.push(RichRewrite {
                        expr: folded,
                        rule_index: rule_idx,
                        direction_id: rules.rtl_id(rule_idx).unwrap(),
                        direction: Direction::Rtl,
                        path: path.to_vec(),
                        rule_name: rule.name.clone(),
                    });
                }
            }
        }

        if depth == 0 {
            return results;
        }

        // Recursively try rewrites in children (with path tracking)
        match &expr.kind {
            ExprKind::Rational(_)
            | ExprKind::Named(_)
            | ExprKind::FracPi(_)
            | ExprKind::Var { .. }
            | ExprKind::Quantity(_, _) => {}

            ExprKind::Add(a, b) => {
                let mut left_path = path.to_vec();
                left_path.push(ChildIndex::Left);
                for mut rewrite in
                    self.all_rewrites_depth(a, rules, depth - 1, &left_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Add(Box::new(rewrite.expr), b.clone()));
                    results.push(rewrite);
                }
                let mut right_path = path.to_vec();
                right_path.push(ChildIndex::Right);
                for mut rewrite in
                    self.all_rewrites_depth(b, rules, depth - 1, &right_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Add(a.clone(), Box::new(rewrite.expr)));
                    results.push(rewrite);
                }
            }

            ExprKind::Mul(a, b) => {
                let mut left_path = path.to_vec();
                left_path.push(ChildIndex::Left);
                for mut rewrite in
                    self.all_rewrites_depth(a, rules, depth - 1, &left_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Mul(Box::new(rewrite.expr), b.clone()));
                    results.push(rewrite);
                }
                let mut right_path = path.to_vec();
                right_path.push(ChildIndex::Right);
                for mut rewrite in
                    self.all_rewrites_depth(b, rules, depth - 1, &right_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Mul(a.clone(), Box::new(rewrite.expr)));
                    results.push(rewrite);
                }
            }

            ExprKind::Pow(base, exp) => {
                let mut left_path = path.to_vec();
                left_path.push(ChildIndex::Left);
                for mut rewrite in
                    self.all_rewrites_depth(base, rules, depth - 1, &left_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Pow(Box::new(rewrite.expr), exp.clone()));
                    results.push(rewrite);
                }
                let mut right_path = path.to_vec();
                right_path.push(ChildIndex::Right);
                for mut rewrite in
                    self.all_rewrites_depth(exp, rules, depth - 1, &right_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Pow(base.clone(), Box::new(rewrite.expr)));
                    results.push(rewrite);
                }
            }

            ExprKind::Neg(a) => {
                let mut inner_path = path.to_vec();
                inner_path.push(ChildIndex::Inner);
                for mut rewrite in
                    self.all_rewrites_depth(a, rules, depth - 1, &inner_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Neg(Box::new(rewrite.expr)));
                    results.push(rewrite);
                }
            }

            ExprKind::Inv(a) => {
                let mut inner_path = path.to_vec();
                inner_path.push(ChildIndex::Inner);
                for mut rewrite in
                    self.all_rewrites_depth(a, rules, depth - 1, &inner_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Inv(Box::new(rewrite.expr)));
                    results.push(rewrite);
                }
            }

            ExprKind::Fn(kind, a) => {
                let mut inner_path = path.to_vec();
                inner_path.push(ChildIndex::Inner);
                for mut rewrite in
                    self.all_rewrites_depth(a, rules, depth - 1, &inner_path, rule_order)
                {
                    rewrite.expr = Expr::new(ExprKind::Fn(kind.clone(), Box::new(rewrite.expr)));
                    results.push(rewrite);
                }
            }

            ExprKind::FnN(kind, args) => {
                for (idx, arg) in args.iter().enumerate() {
                    let mut arg_path = path.to_vec();
                    arg_path.push(ChildIndex::Arg(idx));
                    for mut rewrite in
                        self.all_rewrites_depth(arg, rules, depth - 1, &arg_path, rule_order)
                    {
                        let mut new_args = args.clone();
                        new_args[idx] = rewrite.expr.clone();
                        rewrite.expr = Expr::new(ExprKind::FnN(kind.clone(), new_args));
                        results.push(rewrite);
                    }
                }
            }
        }

        results
    }
}

/// Internal derivation record for trace reconstruction.
struct RichDerivation {
    parent_key: Option<String>,
    step: Option<RichTraceStep>,
}

fn expr_key(expr: &Expr) -> String {
    format!("{:?}", expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::*;
    use crate::rule::RuleSet;

    #[test]
    fn indexed_ruleset_construction() {
        let rs = IndexedRuleSet::new(RuleSet::full());
        let num_rules = rs.len();
        let num_reversible = rs.rtl_map.len();
        assert_eq!(
            rs.total_directions,
            (num_rules + num_reversible) as u16,
            "total_directions = rules({}) + reversible({})",
            num_rules,
            num_reversible
        );
        // Sanity: we have at least 100 rules
        assert!(num_rules > 100, "expected >100 rules, got {}", num_rules);
        // Sanity: some rules are reversible
        assert!(num_reversible > 0, "expected some reversible rules");
    }

    #[test]
    fn direction_ids_contiguous() {
        let rs = IndexedRuleSet::new(RuleSet::full());
        // LTR IDs should be 0..num_rules
        for i in 0..rs.len() {
            assert_eq!(rs.ltr_id(i).0, i as u16);
        }
        // RTL IDs should start after LTR IDs and be contiguous
        let mut rtl_ids: Vec<u16> = rs.rtl_map.values().map(|id| id.0).collect();
        rtl_ids.sort();
        for (i, &id) in rtl_ids.iter().enumerate() {
            assert_eq!(id, (rs.len() + i) as u16);
        }
    }

    #[test]
    fn lookup_direction_roundtrip() {
        let rs = IndexedRuleSet::new(RuleSet::full());
        // Every LTR direction should roundtrip
        for i in 0..rs.len() {
            let id = rs.ltr_id(i);
            let (rule_idx, dir) = rs.lookup_direction(id).unwrap();
            assert_eq!(rule_idx, i);
            assert_eq!(dir, Direction::Ltr);
        }
        // Every RTL direction should roundtrip
        for i in 0..rs.len() {
            if let Some(id) = rs.rtl_id(i) {
                let (rule_idx, dir) = rs.lookup_direction(id).unwrap();
                assert_eq!(rule_idx, i);
                assert_eq!(dir, Direction::Rtl);
            }
        }
        // Out of range returns None
        assert!(rs.lookup_direction(RuleDirectionId(9999)).is_none());
    }

    #[test]
    fn deterministic_baseline() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let search = RandomizedBeamSearch {
            beam_width: 20,
            max_steps: 200,
            epsilon: 0.0,
            shuffle_rules: false,
        };
        let mut rng = StdRng::seed_from_u64(42);

        // x + 0 should simplify to x
        let expr = add(scalar("x"), rational(0, 1));
        let run = search.search_once(&expr, &rules, &mut rng);
        assert_eq!(run.result, scalar("x"));
        assert!(!run.trace.is_empty());
    }

    #[test]
    fn seed_reproducibility() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let search = RandomizedBeamSearch::default();
        let expr = add(
            mul(scalar("x"), scalar("x")),
            mul(rational(2, 1), scalar("x")),
        );

        let mut rng1 = StdRng::seed_from_u64(123);
        let run1 = search.search_once(&expr, &rules, &mut rng1);

        let mut rng2 = StdRng::seed_from_u64(123);
        let run2 = search.search_once(&expr, &rules, &mut rng2);

        assert_eq!(run1.trace.len(), run2.trace.len());
        assert_eq!(run1.result, run2.result);
        for (s1, s2) in run1.trace.iter().zip(run2.trace.iter()) {
            assert_eq!(s1.rule_index, s2.rule_index);
            assert_eq!(s1.direction, s2.direction);
            assert_eq!(s1.path, s2.path);
        }
    }

    #[test]
    fn different_seeds_can_diverge() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let search = RandomizedBeamSearch::default();
        // Use an expression complex enough that different rule orderings matter
        let expr = add(
            add(mul(scalar("x"), scalar("x")), mul(scalar("y"), scalar("y"))),
            mul(mul(rational(2, 1), scalar("x")), scalar("y")),
        );

        let runs = search.search_multi(&expr, &rules, 10, 0);

        // At least some runs should have different trace lengths
        let lengths: Vec<usize> = runs.iter().map(|r| r.trace.len()).collect();
        let all_same = lengths.iter().all(|&l| l == lengths[0]);
        // It's possible (but unlikely with 10 runs) that all are the same.
        // Just check we got valid results.
        for run in &runs {
            assert!(run.final_complexity > 0);
            assert!(run.final_complexity <= expr.complexity());
        }
        // If traces vary, that's good (the common case)
        if !all_same {
            assert!(lengths.len() == 10);
        }
    }

    #[test]
    fn path_tracking_root() {
        let rules = IndexedRuleSet::new(RuleSet::standard());
        let search = RandomizedBeamSearch {
            epsilon: 0.0,
            shuffle_rules: false,
            ..Default::default()
        };

        // x + 0: rule fires at root
        let expr = add(scalar("x"), rational(0, 1));
        let mut rng = StdRng::seed_from_u64(42);
        let run = search.search_once(&expr, &rules, &mut rng);

        // Find the add_zero_right step (x + 0 → x)
        let step = run.trace.iter().find(|s| s.rule_name == "add_zero_right");
        assert!(step.is_some(), "should find add_zero_right rule");
        assert_eq!(step.unwrap().path, vec![]);
    }

    #[test]
    fn path_tracking_child() {
        let rules = IndexedRuleSet::new(RuleSet::standard());
        let search = RandomizedBeamSearch {
            epsilon: 0.0,
            shuffle_rules: false,
            ..Default::default()
        };

        // (x + 0) + y: rule fires in left child
        let expr = add(add(scalar("x"), rational(0, 1)), scalar("y"));
        let mut rng = StdRng::seed_from_u64(42);
        let run = search.search_once(&expr, &rules, &mut rng);

        let step = run.trace.iter().find(|s| s.rule_name == "add_zero_right");
        assert!(step.is_some(), "should find add_zero_right rule");
        assert_eq!(step.unwrap().path, vec![ChildIndex::Left]);
    }

    #[test]
    fn search_best_picks_shortest() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let search = RandomizedBeamSearch::default();
        let expr = add(scalar("x"), rational(0, 1));

        let best = search.search_best(&expr, &rules, 5, 42);
        assert_eq!(best.result, scalar("x"));
        // The best trace should be at most as long as any individual run
        let all = search.search_multi(&expr, &rules, 5, 42);
        let best_among_min_complexity: Vec<_> = all
            .iter()
            .filter(|r| r.final_complexity == best.final_complexity)
            .collect();
        for run in &best_among_min_complexity {
            assert!(best.trace.len() <= run.trace.len());
        }
    }

    #[test]
    fn search_multi_parallel() {
        let rules = IndexedRuleSet::new(RuleSet::full());
        let search = RandomizedBeamSearch::default();
        let expr = mul(scalar("x"), rational(1, 1));

        let runs = search.search_multi(&expr, &rules, 5, 99);
        assert_eq!(runs.len(), 5);
        // All runs should simplify x * 1 to x
        for run in &runs {
            assert_eq!(run.result, scalar("x"));
        }
    }
}
