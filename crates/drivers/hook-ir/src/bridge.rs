//! A library's facts and its constructors' powers read with docket-v2's reader: what a driver's tests hold it to.

use crate::Action;
use docket_v2::syntax::tree::{DeclKind, Lit, Piece, E};

/// The fields of each line `+FACT(…).` of module text `text`, as texts (numbers written as they read).
pub fn facts(text: &str, fact: &str) -> Result<Vec<Vec<String>>, String> {
    let m = docket_v2::syntax::read(text).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for d in m.decls {
        let DeclKind::Holds(x) = d.what else { continue };
        let E::Apply(path, args) = x.e else { continue };
        if path.0.join(".") != fact {
            continue;
        }
        let field = |a: &docket_v2::syntax::tree::Expr| match &a.e {
            E::Text(p) => p.iter().map(|w| if let Piece::Words(s) = w { Ok(s.clone()) } else { Err(format!("{fact}: a field with a hole")) }).collect(),
            E::Lit(Lit::Int(n)) => Ok(n.to_string()),
            other => Err(format!("{fact}: a field that is no text or number: {other:?}")),
        };
        out.push(args.iter().map(field).collect::<Result<_, _>>()?);
    }
    Ok(out)
}

/// The power each constructor of sum type `ty` in module text `text` needs (its `Needs`), by constructor name.
pub fn needs(text: &str, ty: &str) -> Result<Vec<(String, String)>, String> {
    let m = docket_v2::syntax::read(text).map_err(|e| e.to_string())?;
    let variants = m.decls.into_iter().find_map(|d| match d.what {
        DeclKind::Sum { name, variants, .. } if name == ty => Some(variants),
        _ => None,
    });
    Ok(variants.ok_or_else(|| format!("no type {ty}"))?.into_iter().filter_map(|v| Some((v.name, v.needs?))).collect())
}

/// The power each `HookAction` constructor needs, read from std.hooks beside the drivers' crate at `crate_dir`.
pub fn action_needs(crate_dir: &str) -> Vec<(String, String)> {
    let p = format!("{crate_dir}/../../../docket/std/hooks.docket");
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"));
    needs(&text, "HookAction").unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The power action `a` needs, from `needs` as [`needs`] reads it for `HookAction`.
pub fn power(needs: &[(String, String)], a: &Action) -> Option<String> {
    let name = a.named().0;
    needs.iter().find(|(c, _)| c == name).map(|(_, p)| p.clone())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_modules_own_facts_and_its_types_powers_are_read_and_nothing_in_a_comment_is() {
        let t = "module m\nimport std.hooks as H\n// +H.Lets(\"x\", \"0\", \"a\", \"deny\").\n+H.Lets(\"x\", \"1\", \"tool_called\", \"deny\"). +H.Waits(\"x\", \"1\", \"tool_called\", 600).\n";
        assert_eq!(super::facts(t, "H.Lets").unwrap(), vec![vec!["x", "1", "tool_called", "deny"]]);
        assert_eq!(super::facts(t, "H.Waits").unwrap(), vec![vec!["x", "1", "tool_called", "600"]]);
        let h = "module m\ntype A = | Go | Stop(why: String) Needs \"stop\"\n";
        assert_eq!(super::needs(h, "A").unwrap(), vec![("Stop".to_string(), "stop".to_string())]);
        assert!(super::facts("module m\n+F(", "F").is_err());
    }
}
