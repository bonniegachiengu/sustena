//! **A friendlier surface** — many surfaces, one AST (DSL · GENOME §X).
//!
//! ```text
//!   enzyme household.sweep
//!     takes amount: number
//!     when finances.liquid.balance >= params.amount
//!     decrement finances.liquid.balance by params.amount
//!     increment finances.savings by params.amount
//!     emits event.finances.swept
//!     moves money params.amount from "finances.liquid" to "finances.savings"
//! ```
//!
//! ★★★ **This is safe only because DSL-6 holds.** Landin's point is that many
//! surfaces over one core is a feature; many surfaces over *two* cores is two
//! languages wearing one name. This crate has exactly one evaluator and one
//! predicate grammar, so a second way of writing an Enzyme down cannot mean
//! something the first way could not — it parses to the same
//! [`AuthoredEnzyme`], and a test asserts that both surfaces produce a value
//! that is `==`.
//!
//! ★★★ **JSON was the only surface, and JSON is a terrible thing to ask a
//! person to write a rule in.** Quoting, commas, nesting, and a shape that
//! obscures the one thing that matters — *when this, do that*. The row is last
//! in the phase for a good reason: a friendlier surface built before the AST
//! was settled would have been a friendlier way to write the wrong thing.
//!
//! ## Deliberately small
//!
//! ★★ **Line-oriented, no nesting, no lookahead past a line.** Every construct
//! is one line and every line is independent, which makes the parser total by
//! construction and every error report a real line number. A grammar that could
//! nest would need recursion, and recursion is the thing GENOME §XI rules out
//! to keep type-checking decidable.
//!
//! ★★ **The guard is not re-parsed here.** `when ...` hands the rest of the
//! line to [`parse_predicate`](crate::predicate::parse_predicate) untouched. A
//! second predicate parser is precisely the two-evaluator failure DSL-6 names,
//! and it would arrive silently — the same words meaning two things depending
//! on which file they were written in.
//!
//! ★★ **It renders back.** `parse(render(e)) == e` is executed rather than
//! claimed, which is what makes this a *surface* rather than an importer.

use serde_json::Value;

use crate::embroidery::{Action, AuthoredEnzyme, Expr, MovementDecl};
use crate::operator::meta::ParamDecl;
use crate::predicate::parse_predicate;

/// Where a surface text stopped making sense.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceError {
    pub line: usize,
    pub detail: String,
}

impl std::fmt::Display for SurfaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.detail)
    }
}

fn err(line: usize, detail: impl Into<String>) -> SurfaceError {
    SurfaceError { line, detail: detail.into() }
}

/// Read an expression: a quoted string, a number, `params.x`, or a state path.
///
/// ★★ Quoting is what separates a literal from a path, and nothing else does.
/// Guessing — "it looks like a path if it has a dot" — would make
/// `"finances.liquid"` mean different things in two places on one line.
fn expr(token: &str, line: usize) -> Result<Expr, SurfaceError> {
    let t = token.trim();
    if t.is_empty() {
        return Err(err(line, "expected a value here"));
    }
    if let Some(inner) = t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Ok(Expr::Literal(Value::String(inner.to_string())));
    }
    if let Ok(n) = t.parse::<f64>() {
        return Ok(Expr::Literal(crate::predicate::ast::num(n)));
    }
    if let Some(name) = t.strip_prefix("params.") {
        return Ok(Expr::Param(name.to_string()));
    }
    if t == "true" || t == "false" {
        return Ok(Expr::Literal(Value::Bool(t == "true")));
    }
    Ok(Expr::Path(t.to_string()))
}

fn render_expr(e: &Expr) -> String {
    match e {
        Expr::Literal(Value::String(s)) => format!("\"{s}\""),
        Expr::Literal(v) => v.to_string(),
        Expr::Param(name) => format!("params.{name}"),
        Expr::Path(p) => p.clone(),
    }
}

/// Split a line into its keyword and the rest.
fn head(line: &str) -> (&str, &str) {
    match line.split_once(char::is_whitespace) {
        Some((k, rest)) => (k, rest.trim()),
        None => (line, ""),
    }
}

/// **Read an Enzyme from the friendlier surface.**
///
/// ★★★ Every error carries a line number and says what was expected there.
/// "Invalid syntax" for a forty-line document is a message that makes somebody
/// re-read all forty.
pub fn parse_enzyme(text: &str) -> Result<AuthoredEnzyme, Vec<SurfaceError>> {
    let mut errors = Vec::new();
    let mut enzyme: Option<AuthoredEnzyme> = None;

    for (i, raw) in text.lines().enumerate() {
        let no = i + 1;
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (keyword, rest) = head(line);

        if keyword == "enzyme" {
            if enzyme.is_some() {
                errors.push(err(no, "a second 'enzyme' — one document, one Enzyme"));
                continue;
            }
            if rest.is_empty() {
                errors.push(err(no, "'enzyme' needs a name"));
                continue;
            }
            enzyme = Some(AuthoredEnzyme::new(rest));
            continue;
        }

        let Some(e) = enzyme.as_mut() else {
            errors.push(err(no, "everything has to come after 'enzyme <name>'"));
            continue;
        };

        match keyword {
            "takes" => match rest.split_once(':') {
                Some((name, kind)) => {
                    let (name, kind) = (name.trim(), kind.trim());
                    let param = match kind {
                        "number" => ParamDecl::number(name.to_string()),
                        "text" => ParamDecl::text(name.to_string()),
                        other => {
                            errors.push(err(
                                no,
                                format!("'{other}' is not a kind — use number or text"),
                            ));
                            continue;
                        }
                    };
                    e.params.push(param);
                }
                None => errors.push(err(no, "'takes' wants '<name>: number' or '<name>: text'")),
            },

            "when" => {
                // ★★★ Handed to the ONE predicate parser, untouched. A second
                //     one here is the two-evaluator failure DSL-6 names, and it
                //     would arrive silently.
                if let Err(e2) = parse_predicate(rest) {
                    errors.push(err(no, format!("this condition cannot be read: {e2}")));
                } else {
                    e.guard.push(rest.to_string());
                }
            }

            "set" => match rest.split_once('=') {
                Some((path, value)) => match expr(value, no) {
                    Ok(v) => e.effect.push(Action::Set { path: path.trim().into(), value: v }),
                    Err(e2) => errors.push(e2),
                },
                None => errors.push(err(no, "'set' wants '<path> = <value>'")),
            },

            "increment" | "decrement" => match rest.split_once(" by ") {
                Some((path, amount)) => match expr(amount, no) {
                    Ok(by) => {
                        let path = path.trim().to_string();
                        e.effect.push(if keyword == "increment" {
                            Action::Increment { path, by }
                        } else {
                            // ★★ `or below zero` is the surface for
                            //    `allow_negative`. Spelled out because a flag
                            //    that lets money go negative should have to be
                            //    typed, not defaulted.
                            Action::Decrement {
                                path,
                                by,
                                allow_negative: rest.contains("or below zero"),
                            }
                        });
                    }
                    Err(e2) => errors.push(e2),
                },
                None => errors.push(err(no, format!("'{keyword}' wants '<path> by <amount>'"))),
            },

            "emits" => {
                if rest.is_empty() {
                    errors.push(err(no, "'emits' needs an event name"));
                } else {
                    e.emits.push(rest.to_string());
                }
            }

            "moves" => match parse_moves(rest, no) {
                Ok(m) => e.moves.push(m),
                Err(e2) => errors.push(e2),
            },

            other => errors.push(err(no, format!("'{other}' is not something an Enzyme can do"))),
        }
    }

    match enzyme {
        None => {
            errors.push(err(0, "there is no 'enzyme <name>' anywhere in this"));
            Err(errors)
        }
        Some(e) if errors.is_empty() => Ok(e),
        Some(_) => Err(errors),
    }
}

/// `moves <kind> <qty> from <a> to <b>`
fn parse_moves(rest: &str, no: usize) -> Result<MovementDecl, SurfaceError> {
    let (kind, after) = head(rest);
    if kind.is_empty() {
        return Err(err(no, "'moves' wants '<kind> <amount> from <a> to <b>'"));
    }
    let (qty, after) = after
        .split_once(" from ")
        .ok_or_else(|| err(no, "'moves' wants 'from' before where it came from"))?;
    let (from, to) = after
        .split_once(" to ")
        .ok_or_else(|| err(no, "'moves' wants 'to' before where it went"))?;
    Ok(MovementDecl {
        kind: kind.to_string(),
        qty: expr(qty, no)?,
        from: expr(from, no)?,
        to: expr(to, no)?,
    })
}

/// **Write an Enzyme back out.**
///
/// ★★ Round-tripping is what makes this a surface rather than an importer: a
/// person can read what the engine holds, in the same words they would have
/// used to write it.
pub fn render_enzyme(e: &AuthoredEnzyme) -> String {
    let mut out = format!("enzyme {}\n", e.name);
    for p in &e.params {
        let kind = match p.kind {
            crate::operator::meta::ParamKind::Number => "number",
            crate::operator::meta::ParamKind::Text => "text",
            crate::operator::meta::ParamKind::Any => "text",
        };
        out.push_str(&format!("  takes {}: {kind}\n", p.name));
    }
    for g in &e.guard {
        out.push_str(&format!("  when {g}\n"));
    }
    for a in &e.effect {
        match a {
            Action::Set { path, value } => {
                out.push_str(&format!("  set {path} = {}\n", render_expr(value)))
            }
            Action::Increment { path, by } => {
                out.push_str(&format!("  increment {path} by {}\n", render_expr(by)))
            }
            Action::Decrement { path, by, allow_negative } => out.push_str(&format!(
                "  decrement {path} by {}{}\n",
                render_expr(by),
                if *allow_negative { " or below zero" } else { "" }
            )),
        }
    }
    for ev in &e.emits {
        out.push_str(&format!("  emits {ev}\n"));
    }
    for m in &e.moves {
        out.push_str(&format!(
            "  moves {} {} from {} to {}\n",
            m.kind,
            render_expr(&m.qty),
            render_expr(&m.from),
            render_expr(&m.to)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SWEEP: &str = "\
enzyme household.sweep
  takes amount: number
  when finances.liquid.balance >= params.amount
  decrement finances.liquid.balance by params.amount
  increment finances.savings by params.amount
  emits event.finances.swept
";

    #[test]
    fn a_person_can_write_an_enzyme_in_something_readable() {
        let e = parse_enzyme(SWEEP).expect("parses");
        assert_eq!(e.name, "household.sweep");
        assert_eq!(e.params.len(), 1);
        assert_eq!(e.guard, vec!["finances.liquid.balance >= params.amount"]);
        assert_eq!(e.effect.len(), 2);
        assert_eq!(e.emits, vec!["event.finances.swept"]);
    }

    #[test]
    fn the_two_surfaces_produce_the_same_ast() {
        // ★★★ The theorem, executed. Many surfaces over ONE core is a feature;
        //     many surfaces over two cores is two languages wearing one name.
        let from_text = parse_enzyme(SWEEP).expect("parses");
        let built = AuthoredEnzyme::new("household.sweep")
            .taking(ParamDecl::number("amount"))
            .guarded_by("finances.liquid.balance >= params.amount")
            .doing(Action::Decrement {
                path: "finances.liquid.balance".into(),
                by: Expr::Param("amount".into()),
                allow_negative: false,
            })
            .doing(Action::Increment {
                path: "finances.savings".into(),
                by: Expr::Param("amount".into()),
            })
            .emitting("event.finances.swept");
        assert_eq!(from_text, built);
    }

    #[test]
    fn what_it_renders_it_can_read_back() {
        // ★★ What makes this a surface rather than an importer.
        let e = parse_enzyme(SWEEP).expect("parses");
        let round = parse_enzyme(&render_enzyme(&e)).expect("re-parses");
        assert_eq!(e, round);
    }

    #[test]
    fn a_movement_round_trips_including_its_ends() {
        let text = "\
enzyme give
  takes amount: number
  moves money params.amount from \"finances.liquid\" to \"finances.savings\"
";
        let e = parse_enzyme(text).expect("parses");
        assert_eq!(e.moves.len(), 1);
        assert_eq!(e.moves[0].kind, "money");
        assert_eq!(parse_enzyme(&render_enzyme(&e)).expect("re-parses"), e);
    }

    #[test]
    fn quoting_is_what_separates_a_literal_from_a_path() {
        // ★★★ Nothing else does. Guessing — "it looks like a path if it has a
        //     dot" — would make one string mean two things on one line.
        let text = "\
enzyme label
  set finances.label = \"home\"
  set finances.copy = finances.label
";
        let e = parse_enzyme(text).expect("parses");
        assert!(matches!(&e.effect[0], Action::Set { value: Expr::Literal(_), .. }));
        assert!(matches!(&e.effect[1], Action::Set { value: Expr::Path(_), .. }));
    }

    #[test]
    fn letting_money_go_below_zero_has_to_be_typed_out() {
        // ★★ A flag that permits a negative balance should not be a default.
        let plain = parse_enzyme("enzyme a\n  decrement finances.savings by 5\n").expect("parses");
        assert!(matches!(&plain.effect[0], Action::Decrement { allow_negative: false, .. }));

        let allowed = parse_enzyme(
            "enzyme a\n  decrement finances.savings by 5 or below zero\n",
        )
        .expect("parses");
        assert!(matches!(&allowed.effect[0], Action::Decrement { allow_negative: true, .. }));
    }

    #[test]
    fn a_condition_is_read_by_the_one_predicate_parser() {
        // ★★★ A second predicate parser here is the two-evaluator failure
        //     DSL-6 names, and it would arrive silently — the same words
        //     meaning two things depending on which file they were in.
        let errors = parse_enzyme("enzyme a\n  when finances.balance >>>\n").expect_err("refuses");
        assert_eq!(errors[0].line, 2);
        assert!(errors[0].detail.contains("cannot be read"), "{:?}", errors[0]);
    }

    #[test]
    fn every_error_names_the_line_it_is_on() {
        // ★★ "Invalid syntax" for a forty-line document makes somebody re-read
        //    all forty.
        let errors = parse_enzyme(
            "enzyme a\n  takes amount\n  wobble something\n  emits\n",
        )
        .expect_err("refuses");
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn an_unknown_verb_says_so_rather_than_being_ignored() {
        let errors = parse_enzyme("enzyme a\n  delete everything\n").expect_err("refuses");
        assert!(errors[0].detail.contains("not something an Enzyme can do"));
    }

    #[test]
    fn a_document_with_no_enzyme_in_it_says_that_plainly() {
        let errors = parse_enzyme("  takes amount: number\n").expect_err("refuses");
        assert!(
            errors.iter().any(|e| e.detail.contains("there is no 'enzyme")),
            "{errors:?}",
        );
    }

    #[test]
    fn comments_and_blank_lines_are_allowed() {
        let e = parse_enzyme(
            "# the household's own rule\n\nenzyme a\n\n  emits event.x  # why\n",
        )
        .expect("parses");
        assert_eq!(e.emits, vec!["event.x"]);
    }

    #[test]
    fn what_the_surface_produces_still_faces_every_check() {
        // ★★★ A friendlier way to write it is not a friendlier way to skip
        //     anything. The result is an ordinary AuthoredEnzyme, so the type
        //     judgment refuses a broken one exactly as it would have.
        use crate::schema::{DimType, Schema};
        let schema = Schema::new().declare(
            "finances",
            DimType::Record {
                fields: [("label".to_string(), DimType::Text)].into_iter().collect(),
            },
        );
        let e = parse_enzyme("enzyme a\n  increment finances.label by 1\n").expect("parses");
        assert!(e.typecheck(&schema).is_err(), "still a type error");
    }

    #[test]
    fn a_number_written_plainly_is_a_number() {
        let e = parse_enzyme("enzyme a\n  set finances.n = 5\n").expect("parses");
        assert!(matches!(
            &e.effect[0],
            Action::Set { value: Expr::Literal(v), .. } if v == &json!(5)
        ));
    }
}
