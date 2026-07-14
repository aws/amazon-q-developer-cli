//! Translate a single V2 (CLI) regex into V3 (KAS) Cedar-`like` globs (only `*` is special),
//! classified `lossless` / `lossy` (broadened) / `unconvertible`. One regex may fan out to several
//! globs; an unconvertible pattern emits nothing rather than fabricate an over-broad allow.
//!
//! The regex is parsed into a `regex_syntax` AST and walked structurally, so escaping, classes,
//! alternation, and anchors are the parser's job; a parse failure (including look-around) is
//! unconvertible. KAS matches each shell sub-command independently, so a glob still carrying shell
//! chaining can't match a single command — those branches drop, and an all-chaining pattern is
//! unconvertible.

use std::collections::HashSet;

use regex_syntax::ast::{
    Ast,
    GroupKind,
    RepetitionKind,
    RepetitionRange,
};

/// Three-way fidelity classification for a converted pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegexFidelity {
    Lossless,
    Lossy,
    Unconvertible,
}

/// Result of converting one regex to globs; `globs` is empty iff `fidelity == Unconvertible`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexToGlobResult {
    pub globs: Vec<String>,
    pub fidelity: RegexFidelity,
}

/// `drop_chaining` (shell): discard expanded branches that still chain — dead under KAS
/// tokenization. False for web URLs, where `&`/`;` are legitimate.
#[derive(Debug, Clone, Copy, Default)]
pub struct RegexToGlobOptions {
    pub drop_chaining: bool,
}

/// Cross-product fan-out ceiling; past this the pattern is too broad to convert safely.
const MAX_BRANCHES: usize = 256;

/// One expanded alternative, tracking whether the conversion broadened the match.
#[derive(Clone)]
struct Branch {
    glob: String,
    lossy: bool,
}

/// Sentinel: a node has no safe glob; propagates up to mark the whole pattern unconvertible.
struct Unconvertible;

pub fn regex_to_glob(pattern: &str, options: RegexToGlobOptions) -> RegexToGlobResult {
    let ast = match regex_syntax::ast::parse::Parser::new().parse(pattern) {
        Ok(ast) => ast,
        // Parse failure includes look-around, which the `regex` crate rejects — no glob form.
        Err(_) => return unconvertible(),
    };

    let branches = match convert(&ast) {
        Ok(b) => b,
        Err(Unconvertible) => return unconvertible(),
    };
    if branches.len() > MAX_BRANCHES {
        return unconvertible();
    }

    // Shell: keep chaining-free branches; if every branch still chains, keep them so the
    // safety gate below flags the pattern unconvertible rather than silently dropping the rule.
    let kept: Vec<&Branch> = if options.drop_chaining {
        let standalone: Vec<&Branch> = branches.iter().filter(|b| !contains_chaining(&b.glob)).collect();
        if standalone.is_empty() {
            branches.iter().collect()
        } else {
            standalone
        }
    } else {
        branches.iter().collect()
    };

    let mut seen: HashSet<String> = HashSet::new();
    let mut globs: Vec<String> = Vec::new();
    let mut lossy = false;
    for b in &kept {
        let glob = collapse_stars(&b.glob);
        lossy = lossy || b.lossy;
        if glob.is_empty() || seen.contains(&glob) {
            continue;
        }
        seen.insert(glob.clone());
        globs.push(glob);
    }

    // Drop any glob a sibling already covers (`git diff` under `git diff*`), keeping the first of a
    // mutually-covering pair. A zero-occurrence branch survives only when uncovered (`rm` under
    // `sudo *rm`, whose mid-pattern `*` can't reach the front).
    globs = (0..globs.len())
        .filter(|&i| {
            !(0..globs.len())
                .any(|j| j != i && glob_matches(&globs[j], &globs[i]) && (!glob_matches(&globs[i], &globs[j]) || j < i))
        })
        .map(|i| globs[i].clone())
        .collect();

    // A surviving glob that still carries shell chaining (shell) or a pipe (web) can't match a
    // single command/URL — emitting it as an allow rule would be dangerously broad.
    let is_unsafe = |g: &str| {
        if options.drop_chaining {
            contains_chaining(g)
        } else {
            g.contains('|')
        }
    };
    if globs.is_empty() || globs.iter().any(|g| is_unsafe(g)) {
        return unconvertible();
    }

    RegexToGlobResult {
        globs,
        fidelity: if lossy {
            RegexFidelity::Lossy
        } else {
            RegexFidelity::Lossless
        },
    }
}

fn unconvertible() -> RegexToGlobResult {
    RegexToGlobResult {
        globs: vec![],
        fidelity: RegexFidelity::Unconvertible,
    }
}

/// Walk one AST node into the set of glob alternatives it can match. Alternation fans out; concat
/// takes the cross product of its parts.
fn convert(ast: &Ast) -> Result<Vec<Branch>, Unconvertible> {
    match ast {
        Ast::Empty(_) => Ok(vec![lit("")]),
        Ast::Assertion(a) => {
            use regex_syntax::ast::AssertionKind::{
                EndLine,
                EndText,
                StartLine,
                StartText,
            };
            // Line/text anchors drop losslessly; a word boundary is a constraint no glob can
            // express, so drop it but flag lossy.
            let lossy = !matches!(a.kind, StartLine | EndLine | StartText | EndText);
            Ok(vec![Branch {
                glob: String::new(),
                lossy,
            }])
        },
        Ast::Flags(_) => Ok(vec![Branch {
            glob: String::new(),
            // A case-insensitive (or other) inline flag can't be expressed in a glob.
            lossy: true,
        }]),

        Ast::Literal(l) => Ok(vec![lit(&l.c.to_string())]),

        // `.` broadens to `*`; lossy on its own, folded losslessly when quantified as `.*`/`.+`.
        Ast::Dot(_) => Ok(vec![Branch {
            glob: "*".to_string(),
            lossy: true,
        }]),

        // A negated class (`[^…]`, `\D`) is a per-token "not these chars" guard KAS enforces, so `*`
        // is exact (lossless); a positive class (`[…]`, `\d`) broadens (lossy).
        Ast::ClassPerl(c) => Ok(vec![class_glob(c.negated)]),
        Ast::ClassBracketed(c) => Ok(vec![class_glob(c.negated)]),
        Ast::ClassUnicode(_) => Ok(vec![Branch {
            glob: "*".to_string(),
            lossy: true,
        }]),

        Ast::Group(g) => {
            let mut inner = convert(&g.ast)?;
            // A scoped inline flag (`(?i:…)`) can't be expressed as a glob — flag lossy.
            if matches!(&g.kind, GroupKind::NonCapturing(flags) if !flags.items.is_empty()) {
                for b in &mut inner {
                    b.lossy = true;
                }
            }
            Ok(inner)
        },

        Ast::Alternation(alt) => {
            let mut out: Vec<Branch> = Vec::new();
            for a in &alt.asts {
                out.extend(convert(a)?);
                if out.len() > MAX_BRANCHES {
                    return Err(Unconvertible);
                }
            }
            Ok(out)
        },

        Ast::Concat(concat) => {
            let mut acc: Vec<Branch> = vec![lit("")];
            for part in &concat.asts {
                let next = convert(part)?;
                acc = cross(&acc, &next)?;
            }
            Ok(acc)
        },

        Ast::Repetition(rep) => convert_repetition(rep),
    }
}

/// Convert a quantified node. `.*` / `.+` fold to a single lossless `*`; every other quantifier
/// broadens (lossy). A quantifier whose lower bound is zero (`?`, `*`, `{0,n}`) can match nothing,
/// so it also fans out a zero-occurrence branch — dropping it would *narrow* the match (for a deny
/// that is unsafe: it would stop denying the un-prefixed command).
fn convert_repetition(rep: &regex_syntax::ast::Repetition) -> Result<Vec<Branch>, Unconvertible> {
    // `.*` / `.+` is the canonical lossless wildcard; a bare `.` on its own is lossy, so decide
    // losslessness from the repetition rather than the dot node.
    let child_is_dot = matches!(rep.ast.as_ref(), Ast::Dot(_));
    let child = convert(&rep.ast)?;

    let (repeatable, matches_empty) = match rep.op.kind {
        // `x?`: zero or one — the child alone is the "one" case; empty added below.
        RepetitionKind::ZeroOrOne => (false, true),
        RepetitionKind::ZeroOrMore => (true, true),
        RepetitionKind::OneOrMore => (true, false),
        RepetitionKind::Range(RepetitionRange::Exactly(n)) => (n > 1, n == 0),
        RepetitionKind::Range(RepetitionRange::AtLeast(n)) => (true, n == 0),
        RepetitionKind::Range(RepetitionRange::Bounded(lo, hi)) => (hi > 1, lo == 0),
    };

    // The "one or more occurrences" glob for the child.
    let mut out = if !repeatable {
        // `?` and `{1}` / `{0,1}`: exactly the child, no extra wildcard.
        child
    } else if child_is_dot {
        // `.*` / `.+`: canonical lossless wildcard.
        vec![lit("*")]
    } else {
        collapse_repeat(child)
    };

    // A zero-lower-bound quantifier also matches with the child absent.
    if matches_empty {
        out.push(lit(""));
    }
    Ok(out)
}

/// Collapse a repetition of a non-dot child. A child that is already a bare wildcard (`*`, from a
/// class) repeats to itself carrying its own lossiness (so a negated class stays lossless); any
/// other child broadens its literal run to `<child>*` (lossy).
fn collapse_repeat(branches: Vec<Branch>) -> Vec<Branch> {
    branches
        .into_iter()
        .map(|b| {
            if b.glob == "*" {
                b
            } else {
                Branch {
                    glob: format!("{}*", b.glob),
                    lossy: true,
                }
            }
        })
        .collect()
}

/// Cross product of two branch sets: every left glob concatenated with every right glob.
fn cross(left: &[Branch], right: &[Branch]) -> Result<Vec<Branch>, Unconvertible> {
    let mut out: Vec<Branch> = Vec::with_capacity(left.len() * right.len());
    for l in left {
        for r in right {
            out.push(Branch {
                glob: format!("{}{}", l.glob, r.glob),
                lossy: l.lossy || r.lossy,
            });
            if out.len() > MAX_BRANCHES {
                return Err(Unconvertible);
            }
        }
    }
    Ok(out)
}

fn lit(s: &str) -> Branch {
    Branch {
        glob: s.to_string(),
        lossy: false,
    }
}

/// A character class as a glob: always `*`, lossless when negated (native separator guard) and
/// lossy when positive (a real broadening of the allowed set).
fn class_glob(negated: bool) -> Branch {
    Branch {
        glob: "*".to_string(),
        lossy: !negated,
    }
}

/// Shell-chaining operators; a glob containing one can't match a single command.
fn contains_chaining(g: &str) -> bool {
    g.contains('|') || g.contains(';') || g.contains('&')
}

/// Does glob `pat` match string `text` under Cedar `like` semantics (only `*` is special, matching
/// any run including empty)? Used to prune globs a broader sibling already covers.
fn glob_matches(pat: &str, text: &str) -> bool {
    // Standard two-pointer wildcard match with backtracking on `*`.
    let (p, t): (Vec<char>, Vec<char>) = (pat.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Collapse runs of `*` (from adjacent wildcards) into one.
fn collapse_stars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_star = false;
    for c in s.chars() {
        if c == '*' {
            if !prev_star {
                out.push('*');
            }
            prev_star = true;
        } else {
            out.push(c);
            prev_star = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(p: &str) -> RegexToGlobResult {
        regex_to_glob(p, RegexToGlobOptions { drop_chaining: true })
    }

    fn web(p: &str) -> RegexToGlobResult {
        regex_to_glob(p, RegexToGlobOptions { drop_chaining: false })
    }

    fn sorted(r: &RegexToGlobResult) -> Vec<String> {
        let mut g = r.globs.clone();
        g.sort();
        g
    }

    fn lossless(globs: &[&str]) -> RegexToGlobResult {
        RegexToGlobResult {
            globs: globs.iter().map(|s| s.to_string()).collect(),
            fidelity: RegexFidelity::Lossless,
        }
    }

    fn lossy(globs: &[&str]) -> RegexToGlobResult {
        RegexToGlobResult {
            globs: globs.iter().map(|s| s.to_string()).collect(),
            fidelity: RegexFidelity::Lossy,
        }
    }

    fn unconv() -> RegexToGlobResult {
        RegexToGlobResult {
            globs: vec![],
            fidelity: RegexFidelity::Unconvertible,
        }
    }

    // ---- lossless ----

    #[test]
    fn plain_literal_unchanged() {
        // Exact, not `brazil-build*`: V2 fully anchors command patterns (`^…$`), so an unanchored
        // literal matches only the exact command — the exact glob preserves that (not a narrowing).
        assert_eq!(shell("brazil-build"), lossless(&["brazil-build"]));
    }

    #[test]
    fn anchored_literal_drops_anchors() {
        assert_eq!(shell("^git status$"), lossless(&["git status"]));
    }

    #[test]
    fn dot_star_and_dot_plus_become_star() {
        assert_eq!(shell("git diff.*"), lossless(&["git diff*"]));
        assert_eq!(shell(".*rm -rf.*"), lossless(&["*rm -rf*"]));
    }

    #[test]
    fn escaped_literals_stay_literal() {
        assert_eq!(
            shell(r"^cargo \+nightly fmt[^&;]*$"),
            lossless(&["cargo +nightly fmt*"])
        );
    }

    #[test]
    fn negated_class_star_is_lossless_wildcard() {
        assert_eq!(shell(r"^git diff[^&;|]*$"), lossless(&["git diff*"]));
    }

    #[test]
    fn alternation_expands_per_branch() {
        let r = shell("cargo (build|test|check).*");
        assert_eq!(r.fidelity, RegexFidelity::Lossless);
        assert_eq!(sorted(&r), vec!["cargo build*", "cargo check*", "cargo test*"]);
    }

    #[test]
    fn non_capturing_group_expands_like_plain() {
        let r = shell("git (?:status|log).*");
        assert_eq!(r.fidelity, RegexFidelity::Lossless);
        assert_eq!(sorted(&r), vec!["git log*", "git status*"]);
    }

    #[test]
    fn optional_group_expands_with_without() {
        let r = shell("grep( .*)?");
        assert_eq!(r.fidelity, RegexFidelity::Lossless);
        assert_eq!(sorted(&r), vec!["grep", "grep *"]);
    }

    #[test]
    fn optional_chaining_prefix_keeps_standalone() {
        let r = shell("^(cd [^ ]+ && )?(git status)( [^;|&`$]+)?$");
        assert_eq!(r.fidelity, RegexFidelity::Lossless);
        assert_eq!(sorted(&r), vec!["git status", "git status *"]);
    }

    // ---- lossy ----

    #[test]
    fn positive_char_class_quantifier_broadens() {
        assert_eq!(shell("sleep [0-9]+"), lossy(&["sleep *"]));
    }

    #[test]
    fn flag_char_class_broadens() {
        assert_eq!(shell("grep -[rniElw]* .*"), lossy(&["grep -* *"]));
    }

    #[test]
    fn bare_dot_broadens() {
        assert_eq!(shell("ls ."), lossy(&["ls *"]));
    }

    #[test]
    fn shorthand_broadens() {
        assert_eq!(shell(r"sleep \d+"), lossy(&["sleep *"]));
    }

    #[test]
    fn leading_flag_is_lossy() {
        assert_eq!(shell("(?i)^make$"), lossy(&["make"]));
    }

    #[test]
    fn scoped_inline_flag_is_lossy() {
        // `(?i:rm)` can't be expressed as a glob — must be flagged, not silently exact.
        assert_eq!(shell("(?i:rm)"), lossy(&["rm"]));
    }

    #[test]
    fn word_boundary_is_lossy() {
        // `\b` is a real constraint no glob expresses; dropping it must be flagged lossy, not exact.
        assert_eq!(shell(r"\bfoo\b"), lossy(&["foo"]));
    }

    #[test]
    fn optional_prefix_group_keeps_zero_occurrence_branch() {
        // A prefix `*` quantifier must still match the un-prefixed command: `(sudo )*rm` matches
        // `rm`, so `rm` must survive (dropping it would narrow a deny into re-granting `rm`).
        let r = shell("(sudo )*rm");
        assert!(r.globs.contains(&"rm".to_string()), "got {:?}", r.globs);
    }

    // ---- unconvertible ----

    #[test]
    fn lookahead_unconvertible() {
        assert_eq!(shell("^git (?!push).*$"), unconv());
    }

    #[test]
    fn mandatory_chaining_unconvertible() {
        assert_eq!(shell("^foo && bar$"), unconv());
    }

    #[test]
    fn pipe_to_grep_unconvertible() {
        assert_eq!(shell(r"^cat [^;|&`$]+ \| grep [^;|&`$]+$"), unconv());
    }

    #[test]
    fn monster_grammar_unconvertible() {
        let p = r"(?s)^git (-P |--no-pager )*(status|log|diff|show)( [^;|&`$]+)?( 2>(&1|/dev/null))?( \| (tail|head)( -n [0-9]+)?| \| grep( [^;&`$]+)?)?$";
        assert_eq!(shell(p), unconv());
    }

    // ---- web_fetch ----

    #[test]
    fn escaped_dot_domain_converts() {
        assert_eq!(web(r".*docs\.aws\.amazon\.com.*"), lossless(&["*docs.aws.amazon.com*"]));
    }

    #[test]
    fn web_keeps_ampersand() {
        assert_eq!(
            web(r".*example\.com/x\?a=1&b=2.*"),
            lossless(&["*example.com/x?a=1&b=2*"])
        );
    }

    // ---- literal/space `*` quantifier is lossy, not lossless ----

    #[test]
    fn space_quantifier_fans_out_zero_occurrence() {
        // `git ` + `*`-on-space also matches bare `git` (zero spaces) — both branches emitted, and
        // `git` is NOT subsumed by `git *` (the latter requires a literal space).
        assert_eq!(shell("git *"), lossy(&["git *", "git"]));
        assert_eq!(shell("ls *"), lossy(&["ls *", "ls"]));
    }

    #[test]
    fn literal_quantifier_fans_out_zero_occurrence() {
        // `ad` + `d*` also matches `ad` (zero trailing `d`s).
        assert_eq!(shell("add*"), lossy(&["add*", "ad"]));
    }

    #[test]
    fn dot_star_stays_lossless() {
        assert_eq!(shell("cat .*"), lossless(&["cat *"]));
        assert_eq!(shell("git diff.*"), lossless(&["git diff*"]));
    }

    // ---- safety: an AST walk can never emit raw regex syntax ----

    #[test]
    fn never_leaks_raw_regex_syntax() {
        let patterns = [
            "cargo (build|test).*",
            "grep( .*)?",
            "^(a|b|c)+$",
            "x{2,3}",
            "(?s)^git (status|log)( [^;|&`$]+)?$",
        ];
        for p in patterns {
            for g in regex_to_glob(p, RegexToGlobOptions { drop_chaining: true }).globs {
                assert!(
                    !g.contains('(') && !g.contains(')') && !g.contains('^') && !g.contains('$') && !g.contains('\\'),
                    "glob {g:?} from {p:?} leaked regex syntax"
                );
            }
        }
    }
}
