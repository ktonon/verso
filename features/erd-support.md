# ERD Support

## Goal

Support the Emergent Rung Dynamics (ERD) paper — a mathematical physics paper that serves as a provisional implementation of the ERMS framework. ERD models wave dynamics in elastic media, confinement conditions, energy balance, and scale-recursive relations. Verso's current symbolic engine handles algebraic identities, trigonometric relations, and dimensional analysis, but ERD will require capabilities that don't yet exist.

This feature tracks the verso extensions needed to author and verify ERD's mathematical content.

## Dependencies

- **[unicode-completions](unicode-completions.md)**: Landed. ERD now uses `math` tags for simple Greek letter variables (`σ`, `μ`, `K`, `ρ_{0}`).

## Needed Capabilities

### Priority 1: Derivatives and differential equations

ERD's core object is a wave equation in an elastic medium. At minimum, verso needs:

- **Partial derivative notation**: `∂f/∂x`, `∂²f/∂x²` — representable in the AST and renderable to LaTeX
- **Ordinary derivatives**: `df/dx`, `d²f/dx²`
- **Claiming PDE solutions**: e.g. asserting that a given function satisfies a wave equation
- **Verification strategy**: likely dimensional consistency of derivative expressions + numerical spot-check of claimed solutions at sample points

Without this, the paper cannot state its most fundamental equations.

### Priority 2: Inequalities and threshold conditions

Confinement and stability conditions are expressed as inequalities:

- **Inequality claims**: `lhs > rhs`, `lhs >= rhs`, `lhs < rhs`
- **Verification strategy**: numerical sampling (similar to current equality fallback) — verify the inequality holds across random variable samples in a specified domain
- **Domain annotations**: ability to constrain variable ranges (e.g., `!var x > 0` or `!var x [0, inf)`)

### Priority 3: Vector and tensor operators

Forces, flux, and field equations require:

- **Gradient, divergence, curl**: `∇f`, `∇·F`, `∇×F` — at minimum as notation that renders correctly to LaTeX
- **Dot and cross products**: `a·b`, `a×b`
- **Verification strategy**: dimensional analysis of vector expressions (e.g., divergence of a vector field reduces spatial dimension by 1)
- **Existing tensor index support** may partially cover this, but explicit vector calculus operators are more natural for field equations

### Priority 4: Integrals and summations

Energy density, flux, and conservation laws involve:

- **Definite integrals**: `∫_a^b f(x) dx`
- **Surface/volume integrals**: at minimum as notation
- **Summation notation**: `Σ_{n=0}^{N} a_n`
- **Verification strategy**: numerical quadrature for definite integrals; dimensional analysis for all integral/sum expressions

### Priority 5: Piecewise and conditional expressions

Threshold transitions (e.g., confinement onset) may need:

- **Piecewise functions**: `f(x) = { expr1 if cond, expr2 otherwise }`
- **Case-based claims**: different identities hold in different regimes

## Plan

To be determined as ERD work reveals which capabilities are needed first. The priorities above reflect expected ordering based on the paper's structure (wave equations come before stability analysis, which comes before conservation integrals).

Each capability should be designed to integrate with verso's existing verification pipeline:
1. AST representation (new `ExprKind` variants or syntax)
2. LaTeX rendering
3. Dimensional analysis (extend `elaborate_expr`)
4. Verification (symbolic where possible, numerical fallback)

## Implementation Notes

Not yet started. This feature will be updated as ERD authoring proceeds and specific gaps are encountered.

## Verification

- ERD paper claims that exercise each new capability pass `verso check`
- Existing tests continue to pass (no regressions)
- LaTeX output renders correctly for new notation
