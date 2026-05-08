/// Unicode completion table for mathematical symbols.
///
/// Each entry maps a short name to a unicode character and its LaTeX equivalent.
/// Names follow the GitHub/Slack/Discord `:name:` convention.

#[derive(Debug, Clone, Copy)]
pub struct UnicodeEntry {
    pub name: &'static str,
    pub char: char,
    pub latex: &'static str,
}

/// Sorted by name for binary search.
static TABLE: &[UnicodeEntry] = &[
    // Greek lowercase
    UnicodeEntry { name: "alpha", char: 'α', latex: "\\alpha" },
    UnicodeEntry { name: "approx", char: '≈', latex: "\\approx" },
    UnicodeEntry { name: "bbP", char: 'ℙ', latex: "\\mathbb{P}" },
    UnicodeEntry { name: "beta", char: 'β', latex: "\\beta" },
    UnicodeEntry { name: "cdot", char: '·', latex: "\\cdot" },
    UnicodeEntry { name: "chi", char: 'χ', latex: "\\chi" },
    UnicodeEntry { name: "dagger", char: '†', latex: "\\dagger" },
    UnicodeEntry { name: "delta", char: 'δ', latex: "\\delta" },
    UnicodeEntry { name: "ell", char: 'ℓ', latex: "\\ell" },
    UnicodeEntry { name: "epsilon", char: 'ε', latex: "\\epsilon" },
    UnicodeEntry { name: "equiv", char: '≡', latex: "\\equiv" },
    UnicodeEntry { name: "eta", char: 'η', latex: "\\eta" },
    UnicodeEntry { name: "exists", char: '∃', latex: "\\exists" },
    UnicodeEntry { name: "forall", char: '∀', latex: "\\forall" },
    UnicodeEntry { name: "gamma", char: 'γ', latex: "\\gamma" },
    UnicodeEntry { name: "geq", char: '≥', latex: "\\geq" },
    UnicodeEntry { name: "hbar", char: 'ℏ', latex: "\\hbar" },
    UnicodeEntry { name: "iff", char: '⇔', latex: "\\Leftrightarrow" },
    UnicodeEntry { name: "implies", char: '⇒', latex: "\\Rightarrow" },
    UnicodeEntry { name: "in", char: '∈', latex: "\\in" },
    UnicodeEntry { name: "inf", char: '∞', latex: "\\infty" },
    UnicodeEntry { name: "infinity", char: '∞', latex: "\\infty" },
    UnicodeEntry { name: "integral", char: '∫', latex: "\\int" },
    UnicodeEntry { name: "iota", char: 'ι', latex: "\\iota" },
    UnicodeEntry { name: "kappa", char: 'κ', latex: "\\kappa" },
    UnicodeEntry { name: "lambda", char: 'λ', latex: "\\lambda" },
    UnicodeEntry { name: "ldots", char: '…', latex: "\\ldots" },
    UnicodeEntry { name: "leftarrow", char: '←', latex: "\\leftarrow" },
    UnicodeEntry { name: "leftrightarrow", char: '↔', latex: "\\leftrightarrow" },
    UnicodeEntry { name: "leq", char: '≤', latex: "\\leq" },
    UnicodeEntry { name: "mapsto", char: '↦', latex: "\\mapsto" },
    UnicodeEntry { name: "mp", char: '∓', latex: "\\mp" },
    UnicodeEntry { name: "mu", char: 'μ', latex: "\\mu" },
    UnicodeEntry { name: "nabla", char: '∇', latex: "\\nabla" },
    UnicodeEntry { name: "neq", char: '≠', latex: "\\neq" },
    UnicodeEntry { name: "notin", char: '∉', latex: "\\notin" },
    UnicodeEntry { name: "nu", char: 'ν', latex: "\\nu" },
    UnicodeEntry { name: "omega", char: 'ω', latex: "\\omega" },
    UnicodeEntry { name: "parallel", char: '∥', latex: "\\parallel" },
    UnicodeEntry { name: "partial", char: '∂', latex: "\\partial" },
    UnicodeEntry { name: "perp", char: '⟂', latex: "\\perp" },
    UnicodeEntry { name: "phi", char: 'φ', latex: "\\phi" },
    UnicodeEntry { name: "pi", char: 'π', latex: "\\pi" },
    UnicodeEntry { name: "pm", char: '±', latex: "\\pm" },
    UnicodeEntry { name: "prod", char: '∏', latex: "\\prod" },
    UnicodeEntry { name: "psi", char: 'ψ', latex: "\\psi" },
    UnicodeEntry { name: "rho", char: 'ρ', latex: "\\rho" },
    UnicodeEntry { name: "rightarrow", char: '→', latex: "\\rightarrow" },
    UnicodeEntry { name: "section", char: '§', latex: "\\S" },
    UnicodeEntry { name: "sigma", char: 'σ', latex: "\\sigma" },
    UnicodeEntry { name: "sqrt", char: '√', latex: "\\sqrt" },
    UnicodeEntry { name: "subset", char: '⊂', latex: "\\subset" },
    UnicodeEntry { name: "sum", char: '∑', latex: "\\sum" },
    UnicodeEntry { name: "supset", char: '⊃', latex: "\\supset" },
    UnicodeEntry { name: "tau", char: 'τ', latex: "\\tau" },
    UnicodeEntry { name: "theta", char: 'θ', latex: "\\theta" },
    UnicodeEntry { name: "times", char: '×', latex: "\\times" },
    UnicodeEntry { name: "to", char: '→', latex: "\\rightarrow" },
    UnicodeEntry { name: "upsilon", char: 'υ', latex: "\\upsilon" },
    UnicodeEntry { name: "xi", char: 'ξ', latex: "\\xi" },
    UnicodeEntry { name: "zeta", char: 'ζ', latex: "\\zeta" },
    // Greek uppercase, plus Roman numeral characters (typeset upright in math
    // mode). Sorted alphabetically among themselves so the table-sort test
    // stays happy.
    UnicodeEntry { name: "Delta", char: 'Δ', latex: "\\Delta" },
    UnicodeEntry { name: "Gamma", char: 'Γ', latex: "\\Gamma" },
    UnicodeEntry { name: "I", char: 'Ⅰ', latex: "\\mathrm{I}" },
    UnicodeEntry { name: "II", char: 'Ⅱ', latex: "\\mathrm{II}" },
    UnicodeEntry { name: "III", char: 'Ⅲ', latex: "\\mathrm{III}" },
    UnicodeEntry { name: "IV", char: 'Ⅳ', latex: "\\mathrm{IV}" },
    UnicodeEntry { name: "IX", char: 'Ⅸ', latex: "\\mathrm{IX}" },
    UnicodeEntry { name: "Lambda", char: 'Λ', latex: "\\Lambda" },
    UnicodeEntry { name: "Omega", char: 'Ω', latex: "\\Omega" },
    UnicodeEntry { name: "Phi", char: 'Φ', latex: "\\Phi" },
    UnicodeEntry { name: "Pi", char: 'Π', latex: "\\Pi" },
    UnicodeEntry { name: "Psi", char: 'Ψ', latex: "\\Psi" },
    UnicodeEntry { name: "Sigma", char: 'Σ', latex: "\\Sigma" },
    UnicodeEntry { name: "Theta", char: 'Θ', latex: "\\Theta" },
    UnicodeEntry { name: "V", char: 'Ⅴ', latex: "\\mathrm{V}" },
    UnicodeEntry { name: "VI", char: 'Ⅵ', latex: "\\mathrm{VI}" },
    UnicodeEntry { name: "VII", char: 'Ⅶ', latex: "\\mathrm{VII}" },
    UnicodeEntry { name: "VIII", char: 'Ⅷ', latex: "\\mathrm{VIII}" },
    UnicodeEntry { name: "X", char: 'Ⅹ', latex: "\\mathrm{X}" },
    UnicodeEntry { name: "XI", char: 'Ⅺ', latex: "\\mathrm{XI}" },
    UnicodeEntry { name: "XII", char: 'Ⅻ', latex: "\\mathrm{XII}" },
    UnicodeEntry { name: "Xi", char: 'Ξ', latex: "\\Xi" },
];

/// Look up a unicode character by its completion name.
pub fn lookup(name: &str) -> Option<char> {
    TABLE.iter().find(|e| e.name == name).map(|e| e.char)
}

/// Get the LaTeX command for a unicode character.
pub fn to_latex(c: char) -> Option<&'static str> {
    TABLE.iter().find(|e| e.char == c).map(|e| e.latex)
}

/// Replace known unicode math symbols with their LaTeX equivalents.
pub fn replace_unicode_with_latex(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for ch in input.chars() {
        if let Some(latex) = to_latex(ch) {
            result.push_str(latex);
        } else {
            result.push(ch);
        }
    }
    result
}

/// Plain-text-mode equivalent for a Unicode character. Returns the substitution
/// to use when rendering inside ordinary prose (where math commands need to be
/// wrapped in `$...$`, and characters like Roman numerals can simply be ASCII).
/// Returns `None` for characters that should pass through verbatim.
pub fn to_text_latex(c: char) -> Option<&'static str> {
    match c {
        // Blackboard P — wrap math command in inline math.
        'ℙ' => Some("$\\mathbb{P}$"),
        // Roman numerals — emit plain ASCII so they typeset cleanly in
        // section titles and prose without entering math mode.
        'Ⅰ' => Some("I"),
        'Ⅱ' => Some("II"),
        'Ⅲ' => Some("III"),
        'Ⅳ' => Some("IV"),
        'Ⅴ' => Some("V"),
        'Ⅵ' => Some("VI"),
        'Ⅶ' => Some("VII"),
        'Ⅷ' => Some("VIII"),
        'Ⅸ' => Some("IX"),
        'Ⅹ' => Some("X"),
        'Ⅺ' => Some("XI"),
        'Ⅻ' => Some("XII"),
        'ⅰ' => Some("i"),
        'ⅱ' => Some("ii"),
        'ⅲ' => Some("iii"),
        'ⅳ' => Some("iv"),
        'ⅴ' => Some("v"),
        'ⅵ' => Some("vi"),
        'ⅶ' => Some("vii"),
        'ⅷ' => Some("viii"),
        'ⅸ' => Some("ix"),
        'ⅹ' => Some("x"),
        'ⅺ' => Some("xi"),
        'ⅻ' => Some("xii"),
        _ => None,
    }
}

/// Return all entries whose name starts with the given prefix.
pub fn completions(prefix: &str) -> Vec<(&'static str, char)> {
    TABLE
        .iter()
        .filter(|e| e.name.starts_with(prefix))
        .map(|e| (e.name, e.char))
        .collect()
}

/// Scan input for `:name:` patterns and replace with unicode characters.
pub fn replace_all(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find(':') {
        result.push_str(&rest[..start]);
        let after_colon = &rest[start + 1..];
        if let Some(end) = after_colon.find(':') {
            let name = &after_colon[..end];
            if let Some(ch) = lookup(name) {
                result.push(ch);
                rest = &after_colon[end + 1..];
            } else {
                // Not a known name — keep the first colon and continue
                result.push(':');
                rest = after_colon;
            }
        } else {
            // No closing colon — keep everything
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_within_case_groups() {
        // Lowercase entries (a-z start) should be sorted among themselves
        let lowercase: Vec<&str> = TABLE
            .iter()
            .filter(|e| e.name.starts_with(|c: char| c.is_ascii_lowercase()))
            .map(|e| e.name)
            .collect();
        let mut sorted = lowercase.clone();
        sorted.sort();
        assert_eq!(lowercase, sorted, "lowercase entries not sorted");

        // Uppercase entries should be sorted among themselves
        let uppercase: Vec<&str> = TABLE
            .iter()
            .filter(|e| e.name.starts_with(|c: char| c.is_ascii_uppercase()))
            .map(|e| e.name)
            .collect();
        let mut sorted = uppercase.clone();
        sorted.sort();
        assert_eq!(uppercase, sorted, "uppercase entries not sorted");
    }

    // -- lookup --

    #[test]
    fn lookup_greek_lowercase() {
        assert_eq!(lookup("mu"), Some('μ'));
        assert_eq!(lookup("alpha"), Some('α'));
        assert_eq!(lookup("omega"), Some('ω'));
    }

    #[test]
    fn lookup_greek_uppercase() {
        assert_eq!(lookup("Delta"), Some('Δ'));
        assert_eq!(lookup("Sigma"), Some('Σ'));
    }

    #[test]
    fn lookup_math_operators() {
        assert_eq!(lookup("partial"), Some('∂'));
        assert_eq!(lookup("nabla"), Some('∇'));
        assert_eq!(lookup("inf"), Some('∞'));
        assert_eq!(lookup("hbar"), Some('ℏ'));
        assert_eq!(lookup("parallel"), Some('∥'));
        assert_eq!(lookup("perp"), Some('⟂'));
    }

    #[test]
    fn lookup_arrows() {
        assert_eq!(lookup("to"), Some('→'));
        assert_eq!(lookup("rightarrow"), Some('→'));
        assert_eq!(lookup("implies"), Some('⇒'));
        assert_eq!(lookup("mapsto"), Some('↦'));
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert_eq!(lookup("notaname"), None);
        assert_eq!(lookup(""), None);
    }

    // -- to_latex --

    #[test]
    fn to_latex_greek() {
        assert_eq!(to_latex('μ'), Some("\\mu"));
        assert_eq!(to_latex('α'), Some("\\alpha"));
        assert_eq!(to_latex('Δ'), Some("\\Delta"));
    }

    #[test]
    fn to_latex_math_symbols() {
        assert_eq!(to_latex('∂'), Some("\\partial"));
        assert_eq!(to_latex('∇'), Some("\\nabla"));
        assert_eq!(to_latex('∞'), Some("\\infty"));
        assert_eq!(to_latex('≤'), Some("\\leq"));
        assert_eq!(to_latex('∥'), Some("\\parallel"));
        assert_eq!(to_latex('⟂'), Some("\\perp"));
    }

    #[test]
    fn to_latex_unknown_char() {
        assert_eq!(to_latex('x'), None);
        assert_eq!(to_latex('A'), None);
    }

    #[test]
    fn replace_unicode_with_latex_in_expression() {
        assert_eq!(
            replace_unicode_with_latex("c_n^{∥} and c_n^{⟂}"),
            "c_n^{\\parallel} and c_n^{\\perp}"
        );
    }

    #[test]
    fn math_mode_replaces_blackboard_p_and_roman_numerals() {
        assert_eq!(replace_unicode_with_latex("ℙ"), "\\mathbb{P}");
        assert_eq!(replace_unicode_with_latex("ℙⅠ"), "\\mathbb{P}\\mathrm{I}");
        assert_eq!(replace_unicode_with_latex("Ⅻ"), "\\mathrm{XII}");
    }

    #[test]
    fn text_mode_blackboard_p_wraps_in_inline_math() {
        assert_eq!(to_text_latex('ℙ'), Some("$\\mathbb{P}$"));
    }

    #[test]
    fn text_mode_roman_numerals_use_ascii() {
        assert_eq!(to_text_latex('Ⅰ'), Some("I"));
        assert_eq!(to_text_latex('Ⅻ'), Some("XII"));
        assert_eq!(to_text_latex('ⅰ'), Some("i"));
        assert_eq!(to_text_latex('ⅻ'), Some("xii"));
    }

    #[test]
    fn text_mode_passthrough_for_other_chars() {
        // Greek letters and other math chars pass through in prose; the
        // user is expected to use math\`...\` to typeset them.
        assert_eq!(to_text_latex('α'), None);
        assert_eq!(to_text_latex('a'), None);
    }

    // -- completions --

    #[test]
    fn completions_prefix_search() {
        let results = completions("al");
        assert_eq!(results, vec![("alpha", 'α')]);
    }

    #[test]
    fn completions_multiple_matches() {
        let results = completions("mu");
        assert_eq!(results, vec![("mu", 'μ')]);

        let results = completions("p");
        let names: Vec<&str> = results.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"partial"));
        assert!(names.contains(&"phi"));
        assert!(names.contains(&"pi"));
        assert!(names.contains(&"parallel"));
        assert!(names.contains(&"pm"));
        assert!(names.contains(&"perp"));
        assert!(names.contains(&"prod"));
        assert!(names.contains(&"psi"));
    }

    #[test]
    fn completions_empty_prefix_returns_all_lowercase() {
        let results = completions("");
        assert!(results.len() >= 54); // lowercase entries
    }

    #[test]
    fn completions_no_match() {
        let results = completions("zzz");
        assert!(results.is_empty());
    }

    // -- replace_all --

    #[test]
    fn replace_all_single() {
        assert_eq!(replace_all(":mu:"), "μ");
    }

    #[test]
    fn replace_all_multiple() {
        assert_eq!(replace_all(":alpha: + :beta:"), "α + β");
    }

    #[test]
    fn replace_all_in_expression() {
        assert_eq!(replace_all(":partial:f/:partial:x"), "∂f/∂x");
    }

    #[test]
    fn replace_all_parallel_and_perp() {
        assert_eq!(replace_all(":parallel: and :perp:"), "∥ and ⟂");
    }

    #[test]
    fn replace_all_unknown_preserved() {
        assert_eq!(replace_all(":unknown:"), ":unknown:");
    }

    #[test]
    fn replace_all_no_triggers() {
        assert_eq!(replace_all("plain text"), "plain text");
    }

    #[test]
    fn replace_all_unclosed_colon() {
        assert_eq!(replace_all("a :trailing"), "a :trailing");
    }

    #[test]
    fn replace_all_mixed() {
        assert_eq!(
            replace_all("f(:mu:) = :nabla: g"),
            "f(μ) = ∇ g"
        );
    }

    #[test]
    fn replace_all_adjacent() {
        assert_eq!(replace_all(":alpha::beta:"), "αβ");
    }

    #[test]
    fn replace_all_empty_colons_preserved() {
        assert_eq!(replace_all("::"), "::");
    }
}
