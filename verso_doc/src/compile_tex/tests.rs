use super::*;
use crate::parse::parse_document;

#[test]
fn compile_section() {
    let doc = parse_document("# My Section").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\section{My Section}"));
}

#[test]
fn compile_prose_with_inline_math() {
    let doc = parse_document("The value math`x + 1` is positive.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("The value $x + 1$ is positive."));
}

#[test]
fn compile_inline_math_equality() {
    let doc = parse_document("We define math`a = b + c` here.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("We define $a = b + c$ here."));
}

#[test]
fn compile_var_renders_equation() {
    let src = "var v [L T^-1]\n  Velocity.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains("\\begin{equation}"),
        "var should render as equation: {}",
        tex
    );
    assert!(
        tex.contains("\\end{equation}"),
        "var should close equation: {}",
        tex
    );
    assert!(tex.contains("v"), "should contain variable name: {}", tex);
    assert!(
        tex.contains("\\mathrm{L}"),
        "should contain dimension: {}",
        tex
    );
    assert!(
        tex.contains("Velocity."),
        "should contain description: {}",
        tex
    );
}

#[test]
fn compile_def_renders_equation() {
    let doc = parse_document("def c := 3*10^8").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains("\\begin{equation}"),
        "def should render as equation: {}",
        tex
    );
    assert!(tex.contains(":="), "def should show := operator: {}", tex);
    assert!(
        tex.contains("10^{8}"),
        "def should contain expression: {}",
        tex
    );
}

#[test]
fn compile_def_with_description() {
    let src = "def c := 3*10^8\n  Speed of light.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains("Speed of light."),
        "def should render description: {}",
        tex
    );
}

#[test]
fn compile_prose_with_claim_ref() {
    let doc = parse_document("See claim`pythag` for details.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("See \\eqref{eq:pythag} for details."));
}

#[test]
fn compile_claim() {
    let doc = parse_document("claim foo\n  x + 1 = y").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{equation} \\label{eq:foo}"));
    assert!(tex.contains("\\end{equation}"));
}

#[test]
fn compile_proof() {
    let src = "\
proof expand
  x + 0
  = x             ; add_identity
";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{align*}"));
    assert!(tex.contains("&= x"));
    assert!(tex.contains("\\text{(add_identity)}"));
    assert!(tex.contains("\\end{align*}"));
}

#[test]
fn compile_full_document() {
    let src = "\
# Algebra

claim add_zero
  x + 0 = x
";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\documentclass[11pt]{article}"));
    assert!(tex.contains("\\usepackage{amsmath}"));
    assert!(tex.contains("\\begin{document}"));
    assert!(tex.contains("\\end{document}"));
    assert!(tex.contains("\\section{Algebra}"));
}

#[test]
fn compile_bold_and_italic() {
    let doc = parse_document("This is **bold** and *italic* text.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("This is \\textbf{bold} and \\textit{italic} text."));
}

#[test]
fn compile_bold_italic_combined() {
    let doc = parse_document("This is ***emphasized*** text.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\textbf{\\textit{emphasized}}"));
}

#[test]
fn compile_math_block_single() {
    let doc = parse_document("```math\nx + 1\n```").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\["));
    assert!(tex.contains("x + 1"));
    assert!(tex.contains("\\]"));
}

#[test]
fn compile_math_block_multi() {
    let doc = parse_document("```math\nx + 1\ny + 2\n```").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{gather*}"));
    assert!(tex.contains("x + 1 \\\\"));
    assert!(tex.contains("y + 2"));
    assert!(tex.contains("\\end{gather*}"));
}

#[test]
fn compile_cite() {
    let doc = parse_document("See cite`einstein1905` here.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\cite{einstein1905}"));
}

#[test]
fn compile_bibliography() {
    let doc = parse_document("!bibliography refs.bib").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\bibliographystyle{plain}"));
    assert!(tex.contains("\\bibliography{refs}"));
    let bib_pos = tex.find("\\bibliography{refs}").unwrap();
    let end_pos = tex.find("\\end{document}").unwrap();
    assert!(bib_pos < end_pos);
}

#[test]
fn compile_theorem_with_title() {
    let src = "!theorem Pythagorean\n  For right triangles.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\newtheorem{theorem}{Theorem}"));
    assert!(tex.contains("\\begin{theorem}[Pythagorean]"));
    assert!(tex.contains("For right triangles."));
    assert!(tex.contains("\\end{theorem}"));
}

#[test]
fn compile_newtheorem_only_for_used_kinds() {
    let src = "!lemma\n  Body A.\n\n!lemma Another\n  Body B.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert_eq!(tex.matches("\\newtheorem{lemma}{Lemma}").count(), 1);
    assert!(!tex.contains("\\newtheorem{theorem}"));
}

#[test]
fn compile_amsthm_included() {
    let src = "!theorem\n  Body.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\usepackage{amsthm}"));
}

#[test]
fn compile_env_with_inline_math() {
    let src = "!theorem\n  If math`x` is positive then result holds.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("$x$"));
}

#[test]
fn compile_center() {
    let doc = parse_document("!center\n\tSome centered text.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{center}"));
    assert!(tex.contains("Some centered text."));
    assert!(tex.contains("\\end{center}"));
}

#[test]
fn compile_block_quote() {
    let doc = parse_document("> A famous result.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{quote}"));
    assert!(tex.contains("A famous result."));
    assert!(tex.contains("\\end{quote}"));
}

#[test]
fn compile_footnote() {
    let doc = parse_document("Result^[First noted by Euler.] here.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\footnote{First noted by Euler.}"));
}

#[test]
fn compile_comment_produces_no_output() {
    let doc = parse_document("% This is a comment\nVisible.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(!tex.contains("comment"));
    assert!(tex.contains("Visible."));
}

#[test]
fn compile_section_has_label() {
    let doc = parse_document("# My Section").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\section{My Section}"));
    assert!(tex.contains("\\label{my-section}"));
}

#[test]
fn compile_ref_with_auto_title() {
    let src = "# Newton's Laws\n\nSee ref`newtons-laws` for details.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\hyperref[newtons-laws]{Newton's Laws}"));
    assert!(tex.contains(
        "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{hyperref}"
    ));
}

#[test]
fn compile_ref_with_custom_display() {
    let src = "# Earth and the Solar System\n\nref`earth-and-the-solar-system|Hydrogen creation`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\hyperref[earth-and-the-solar-system]{Hydrogen creation}"));
}

#[test]
fn compile_ref_unresolved_uses_label() {
    let doc = parse_document("See ref`unknown-section` here.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\hyperref[unknown-section]{unknown-section}"));
}

#[test]
fn compile_no_hyperref_without_refs() {
    let doc = parse_document("# My Section\n\nJust prose.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(!tex.contains(
        "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{hyperref}"
    ));
}

#[test]
fn compile_ref_in_bold_in_list() {
    let src =
        "## Earth and the Solar System\n\n1. **ref`earth-and-the-solar-system|Hydrogen creation`** *— abundant*";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\textbf{\\hyperref[earth-and-the-solar-system]{Hydrogen creation}}"));
    assert!(tex.contains("\\textit{— abundant}"));
}

#[test]
fn compile_default_preamble() {
    let src = "Just prose.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\documentclass[11pt]{article}"));
    assert!(tex.contains("\\usepackage[margin=1in]{geometry}"));
    assert!(tex.contains("\\usepackage{amsmath}"));
    assert!(tex.contains("\\usepackage{microtype}"));
    assert!(tex.contains("\\usepackage{lmodern}"));
    assert!(tex.contains("\\setlength{\\parindent}{0pt}"));
    assert!(tex.contains("\\setlength{\\parskip}{6pt plus 2pt minus 1pt}"));
}

#[test]
fn compile_table_full() {
    let src = "!table Results\n  | A | B |\n  |:--|--:|\n  | 1 | 2 |";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{longtable}"));
    assert!(
        tex.contains("\\raggedright\\arraybackslash")
            && tex.contains("\\raggedleft\\arraybackslash"),
        "should use paragraph columns with alignment: {}",
        tex
    );
    assert!(tex.contains("\\textbf{A} & \\textbf{B}"));
    assert!(tex.contains("1 & 2"));
    assert!(tex.contains("\\caption{Results}"));
    assert!(tex.contains("\\end{longtable}"));
}

#[test]
fn compile_table_with_label() {
    let src = "!table T\n  | X |\n  |---|\n  | 1 |\n  label: tab-x";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\label{tab:tab-x}"));
}

#[test]
fn compile_table_no_title() {
    let src = "!table\n  | X |\n  |---|\n  | 1 |";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(!tex.contains("\\caption"));
}

#[test]
fn compile_figure_full() {
    let src = "!figure plots/energy.pdf\n  caption: Energy levels.\n  label: fig-energy\n  width: 0.8";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\usepackage{graphicx}"));
    assert!(tex.contains("\\begin{figure}[H]"));
    assert!(tex.contains("\\centering"));
    assert!(tex.contains("\\includegraphics[width=0.8\\textwidth]{plots/energy.pdf}"));
    assert!(tex.contains("\\caption{Energy levels.}"));
    assert!(tex.contains("\\label{fig:fig-energy}"));
    assert!(tex.contains("\\end{figure}"));
}

#[test]
fn compile_figure_path_only() {
    let src = "!figure img.png";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\includegraphics[width=1\\textwidth]{img.png}"));
    assert!(!tex.contains("\\caption"));
    assert!(!tex.contains("\\label{fig:"));
}

#[test]
fn compile_full_metadata() {
    let src = "!title My Paper\n!author Alice\n!author Bob\n!date 2026\n!abstract\n  Some abstract text.\n\nBody here.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\title{My Paper}"));
    assert!(tex.contains("\\author{Alice \\and Bob}"));
    assert!(tex.contains("\\date{2026}"));
    assert!(tex.contains("\\maketitle"));
    assert!(tex.contains("\\begin{abstract}"));
    assert!(tex.contains("Some abstract text."));
    assert!(tex.contains("\\end{abstract}"));
}

#[test]
fn compile_multiline_title() {
    let src = "!title\n\tLine One\n\tLine Two";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\title{Line One \\\\\nLine Two}"));
}

#[test]
fn compile_date_iso_format() {
    let src = "!title T\n!date 2026-03-14";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\date{March 14, 2026}"));
}

#[test]
fn compile_date_no_value_uses_today() {
    let src = "!title T\n!date";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\date{\\today}"));
}

#[test]
fn compile_no_date_directive_omits_date() {
    let src = "!title T";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(!tex.contains("\\date"));
}

#[test]
fn compile_no_metadata_no_maketitle() {
    let src = "Just some prose.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(!tex.contains("\\maketitle"));
}

#[test]
fn compile_abstract_with_math() {
    let src = "!title T\n!abstract\n  We study math`x^2`.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\begin{abstract}"));
    assert!(tex.contains("$x^{2}$"));
    assert!(tex.contains("\\end{abstract}"));
}

#[test]
fn compile_abstract_paragraph_break() {
    let src = "!title T\n!abstract\n  First paragraph.\n\n  Second paragraph.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("First paragraph.\n\\par\nSecond paragraph."));
}

#[test]
fn compile_tilde_in_prose() {
    let src = "~200 million years and T~5000K";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\textasciitilde{}200 million years and T\\textasciitilde{}5000K"));
}

#[test]
fn compile_quotes_in_heading() {
    let src = r#"## The "Standard" Model"#;
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains(r"\texorpdfstring{The ``Standard'' Model}{The "),
        "smart quotes in heading should use texorpdfstring: {}",
        tex
    );
}

#[test]
fn compile_smart_quotes() {
    let src = r#"He said "hello" and she said "goodbye""#;
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("He said ``hello'' and she said ``goodbye''"));
}

#[test]
fn compile_smart_quotes_unmatched_stays() {
    let src = r#"A lone " on this line"#;
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains(r#"A lone " on this line"#));
}

#[test]
fn compile_smart_quotes_and_tilde() {
    let src = r#"~"both""#;
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\textasciitilde{}``both''"));
}

#[test]
fn compile_toc() {
    let src = "!toc";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\tableofcontents"));
}

#[test]
fn compile_url_plain() {
    let src = "See url`https://example.com`.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\url{https://example.com}"));
    assert!(tex.contains(
        "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{hyperref}"
    ));
}

#[test]
fn compile_url_with_display() {
    let src = "Click url`https://example.com|here`.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\href{https://example.com}{here}"));
}

#[test]
fn compile_pagebreak() {
    let src = "Text.\n\n!pagebreak\n\nMore.";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\newpage"));
}

#[test]
fn unresolved_ref_detected() {
    let src = "## Introduction\n\nSee ref`nonexistent` and ref`introduction`.";
    let doc = parse_document(src).unwrap();
    let unresolved = find_unresolved_refs(&doc);
    assert_eq!(unresolved, vec!["nonexistent"]);
}

#[test]
fn all_refs_resolved() {
    let src = "## Newton's Laws\n\nSee ref`newtons-laws`.";
    let doc = parse_document(src).unwrap();
    let unresolved = find_unresolved_refs(&doc);
    assert!(unresolved.is_empty());
}

#[test]
fn unresolved_ref_figure_label_resolved() {
    let src = "!figure img.png\n  label: my-fig\n\nSee ref`my-fig`.";
    let doc = parse_document(src).unwrap();
    let unresolved = find_unresolved_refs(&doc);
    assert!(unresolved.is_empty());
}

#[test]
fn unresolved_ref_table_label_resolved() {
    let src = "!table T\n  | A |\n  |---|\n  | 1 |\n  label: my-tab\n\nSee ref`my-tab`.";
    let doc = parse_document(src).unwrap();
    let unresolved = find_unresolved_refs(&doc);
    assert!(unresolved.is_empty());
}

#[test]
fn slugify_basic() {
    assert_eq!(slugify("Newton's Laws"), "newtons-laws");
    assert_eq!(slugify("E = mc²"), "e-mc");
    assert_eq!(slugify("The 2nd Law"), "the-2nd-law");
    assert_eq!(
        slugify("Earth and the Solar System"),
        "earth-and-the-solar-system"
    );
    assert_eq!(slugify("  Leading spaces  "), "leading-spaces");
}

#[test]
fn native_label_resolves_ref() {
    let src = "## Long Title label`short`\n\nSee ref`short`.";
    let doc = parse_document(src).unwrap();
    let unresolved = find_unresolved_refs(&doc);
    assert!(unresolved.is_empty());
}

#[test]
fn legacy_backslash_label_resolves_ref() {
    let src = "## Long Title \\label{short}\n\nSee ref`short`.";
    let doc = parse_document(src).unwrap();
    let unresolved = find_unresolved_refs(&doc);
    assert!(unresolved.is_empty());
}

#[test]
fn label_stripped_from_section_title_in_tex() {
    let src = "## Absolute Time label`absolute-time`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("\\subsection{Absolute Time}"));
    assert!(tex.contains("\\label{absolute-time}"));
    assert!(!tex.contains("label`"));
}

#[test]
fn find_label_line_section_slug() {
    let text = "# Introduction\n\nSome text.";
    assert_eq!(find_label_line("introduction", text), Some(1));
}

#[test]
fn find_label_line_section_explicit() {
    let text = "# Long Title label`short`\n\nSome text.";
    assert_eq!(find_label_line("short", text), Some(1));
}

#[test]
fn find_label_line_section_legacy_label() {
    let text = "# Long Title \\label{short}\n\nSome text.";
    assert_eq!(find_label_line("short", text), Some(1));
}

#[test]
fn find_label_line_figure_label() {
    let text = "Some text.\n\n!figure img.png\n  caption: A figure\n  label: my-fig";
    assert_eq!(find_label_line("my-fig", text), Some(3));
}

#[test]
fn find_label_line_table_label() {
    let text = "!table My Table\n  | A |\n  |---|\n  | 1 |\n  label: my-tab";
    assert_eq!(find_label_line("my-tab", text), Some(1));
}

#[test]
fn find_label_line_not_found() {
    let text = "# Introduction\n\nSome text.";
    assert_eq!(find_label_line("nonexistent", text), None);
}

#[test]
fn find_label_line_explicit_over_slug() {
    let text = "# Newton's Laws label`laws`\n\ntext";
    assert_eq!(find_label_line("laws", text), Some(1));
    assert_eq!(find_label_line("newtons-laws", text), Some(1));
}

#[test]
fn find_claim_line_basic() {
    let text = "var x [L]\n\nclaim energy\n  x = x";
    assert_eq!(find_claim_line("energy", text), Some(3));
}

#[test]
fn find_claim_line_not_found() {
    let text = "claim energy\n  x = x";
    assert_eq!(find_claim_line("missing", text), None);
}

#[test]
fn collect_symbols_var() {
    let doc = parse_document("var v [L T^-1]").unwrap();
    let syms = collect_symbols(&doc);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "v");
    assert_eq!(syms[0].kind, "var");
}

#[test]
fn collect_symbols_def() {
    let doc = parse_document("def k := 2").unwrap();
    let syms = collect_symbols(&doc);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "k");
    assert_eq!(syms[0].kind, "def");
    assert_eq!(syms[0].detail, "2");
}

#[test]
fn collect_symbols_func() {
    let doc = parse_document("func sq(x) := x^2").unwrap();
    let syms = collect_symbols(&doc);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "sq");
    assert_eq!(syms[0].kind, "func");
}

#[test]
fn collect_symbols_claim() {
    let doc = parse_document("claim trivial\n  x = x").unwrap();
    let syms = collect_symbols(&doc);
    assert_eq!(syms.len(), 1);
    assert_eq!(syms[0].name, "trivial");
    assert_eq!(syms[0].kind, "claim");
}

#[test]
fn collect_symbols_var_with_description() {
    let doc = parse_document("var v [L T^-1]\n  Velocity.").unwrap();
    let syms = collect_symbols(&doc);
    assert_eq!(syms.len(), 1);
    assert!(syms[0].detail.contains("Velocity."));
}

#[test]
fn compile_sym_var() {
    let src = "var v [L T^-1]\n  Velocity.\n\nHere: sym`v`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(tex.contains("$v$"), "should render symbol as math: {}", tex);
    assert!(
        tex.contains("Velocity."),
        "should include description: {}",
        tex
    );
}

#[test]
fn compile_sym_with_override() {
    let src = "var v [L T^-1]\n  Velocity.\n\nHere: sym`v|Speed of the particle.`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains("Speed of the particle."),
        "should use override: {}",
        tex
    );
    let sym_line = tex
        .lines()
        .find(|line| line.contains("Speed of the particle."))
        .unwrap();
    assert!(
        !sym_line.contains("Velocity."),
        "sym line should not contain declared desc: {}",
        sym_line
    );
}

#[test]
fn compile_sym_prefers_exact_match_over_base() {
    let src = "var ℓ_{n} [L]\n  Characteristic length at rung math`n`.\ndef ℓ_{n-1} := ℓ_{n} / σ\n  Characteristic length scaling\n\n- sym`ℓ_{n-1}`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains("Characteristic length scaling"),
        "sym should resolve to the exact-match def, not the base-match var: {}",
        tex
    );
}

#[test]
fn compile_sym_def_detail_uses_latex() {
    let src = "var ℓ_{n} [L]\ndef ℓ_{n-1} := ℓ_{n} / σ\n\n- sym`ℓ_{n-1}`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        !tex.contains("ℓ"),
        "should not contain raw unicode ℓ in output: {}",
        tex
    );
    assert!(
        !tex.contains("σ"),
        "should not contain raw unicode σ in output: {}",
        tex
    );
}

#[test]
fn compile_sym_unknown() {
    let src = "Here: sym`unknown`";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains("$unknown$"),
        "should still render name as math: {}",
        tex
    );
}

#[test]
fn escape_prose_underscores_and_carets() {
    let doc = parse_document("The expect_fail block has dimension L T^-1.").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains(r"expect\_fail"),
        "underscores should be escaped: {}",
        tex
    );
    assert!(
        tex.contains(r"T\^{}-1"),
        "carets should be escaped: {}",
        tex
    );
}

#[test]
fn escape_prose_in_section_title() {
    let doc = parse_document("## expect_fail Blocks").unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains(r"\texorpdfstring{expect\_fail Blocks}{expect_fail Blocks}"),
        "section titles with special chars should use texorpdfstring: {}",
        tex
    );
}

#[test]
fn escape_prose_in_table_cells() {
    let src = "!table T\n  | Type | Description |\n  |------|-------------|\n  | dimension_mismatch | LHS mismatch |";
    let doc = parse_document(src).unwrap();
    let tex = compile_to_tex(&doc);
    assert!(
        tex.contains(r"dimension\_mismatch"),
        "underscores in table cells should be escaped: {}",
        tex
    );
}
