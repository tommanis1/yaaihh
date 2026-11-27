//! What a phrase's outputs print, as the reference prints them.

use super::*;

pub(super) fn draw_tree(info: &TransInfo, out: &mut Vec<String>) {
    let status = if !info.is_action() {
        ""
    } else if info.disabled {
        "(DISABLED)"
    } else {
        "(ENABLED)"
    };
    out.push(format!("{} {}", info.tagged, status));
    let n = info.syncs.len();
    for (i, s) in info.syncs.iter().enumerate() {
        let mut sub = Vec::new();
        draw_tree(s, &mut sub);
        out.push("|".to_string());
        let (first, other) = if i + 1 == n { ("`- ", "   ") } else { ("+- ", "|  ") };
        for (j, line) in sub.into_iter().enumerate() {
            out.push(format!("{}{}", if j == 0 { first } else { other }, line));
        }
    }
}

/// The text printed for one phrase. In test mode only errors, failed queries and the answers to
/// instance queries are printed.
pub fn report(outs: &[Output], old: &Config, new: &Config, test_mode: bool) -> String {
    let mut s = String::new();
    let line = |s: &mut String, l: &str| {
        s.push_str(l);
        s.push('\n');
    };
    // missing input replaces the whole report
    let mut missing: Vec<&RtErr> = Vec::new();
    for o in outs {
        if let Output::Runtime(e) = o {
            if e.is_missing_input() && !missing.contains(&e) {
                missing.push(e);
            }
        }
    }
    if !missing.is_empty() {
        for e in &missing {
            if let RtErr::MissingInstances(d) = e {
                line(&mut s, &format!("missing input assignments for type: {}", d));
            }
        }
        for e in &missing {
            if let RtErr::MissingAssignments(tes) = e {
                for (te, a) in tes {
                    if *a == Assign::Unknown {
                        line(&mut s, &format!("\nmissing truth-value for: {}", te));
                    }
                }
            }
        }
        return s;
    }
    for o in outs {
        match o {
            Output::CompilationError(errs) => {
                for e in errs {
                    line(&mut s, e);
                }
            }
            Output::NotTriggerable(d) => line(&mut s, &format!("not a triggerable (act- or event-) type: {}", d)),
            Output::Runtime(e) => line(&mut s, &e.message()),
            _ => {}
        }
    }
    for o in outs {
        match o {
            Output::Query(true) if !test_mode => line(&mut s, "query successful"),
            Output::Query(false) => line(&mut s, "query failed"),
            Output::Unknown => line(&mut s, "query unknown"),
            _ => {}
        }
    }
    if new.state.unknown() != old.state.unknown() {
        for u in undecided(&new.spec, &new.state) {
            line(&mut s, &u.message());
        }
    }
    for o in outs {
        if let Output::InstQuery(tes) = o {
            for te in tes {
                line(&mut s, &te.to_string());
            }
            line(&mut s, "");
        }
    }
    if test_mode {
        return s;
    }
    for o in outs {
        if let Output::Executed(info) = o {
            line(&mut s, "executed transition: ");
            let mut lines = Vec::new();
            draw_tree(info, &mut lines);
            for l in lines {
                line(&mut s, &l);
            }
        }
    }
    let violations: Vec<&Violation> =
        outs.iter().filter_map(|o| if let Output::Violation(v) = o { Some(v) } else { None }).collect();
    if !violations.is_empty() {
        line(&mut s, "violations:");
        for v in violations {
            match v {
                Violation::Duty(te) => line(&mut s, &format!("  violated duty!: {}", te)),
                Violation::Invariant(d) => line(&mut s, &format!("  violated invariant!: {}", d)),
                Violation::Trigger(info) => line(
                    &mut s,
                    &format!("  disabled {}: {}", if info.is_action() { "action" } else { "event" }, info.tagged),
                ),
            }
        }
    }
    for d in new.spec.decls.keys().filter(|d| !old.spec.decls.contains_key(*d)) {
        line(&mut s, &format!("New type {}", d));
    }
    for d in old.spec.decls.keys().filter(|d| !new.spec.decls.contains_key(*d)) {
        line(&mut s, &format!("Removed type {}", d));
    }
    for te in old.state.contents().keys().filter(|te| !new.state.contents().contains_key(*te)) {
        line(&mut s, &format!("~{}", te));
    }
    for te in new.state.not_holding().filter(|te| old.state.contents().get(*te).map_or(true, |i| i.value)) {
        line(&mut s, &format!("-{}", te));
    }
    for te in new.state.holding().filter(|te| !old.state.holds(te)) {
        line(&mut s, &format!("+{}", te));
    }
    for te in new.state.unknown().iter().filter(|te| !old.state.unknown().contains(*te)) {
        line(&mut s, &format!("?{}", te));
    }
    let (old_inv, new_inv) = (old.spec.invariants(), new.spec.invariants());
    for d in new_inv.iter().filter(|d| !old_inv.contains(d)) {
        line(&mut s, &format!("New invariant {}", d));
    }
    for d in old_inv.iter().filter(|d| !new_inv.contains(d)) {
        line(&mut s, &format!("Removed invariant {}", d));
    }
    s
}
