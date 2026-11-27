//! A phrase performed on a configuration in place, its derived types worked out only when something reads them.

use super::*;

/// One phrase performed on CFG in place, for a record taken a line at a time: the state is not copied, and a derived
/// type is worked out only when something reads it. STALE holds the derived types whose instances in the state may be
/// out of date. Before the phrase runs, those of them it may read are worked out (`freshen`); an act or event
/// performed is worked out for its own instance only (`Ev::derives`); a phrase that changes a stored fact makes every
/// derived type stale. A phrase that declares types goes through `step_with`. As `step_with`
/// not full, no transition is listed and no violation reported. `false`: not performed, the stored facts as they were.
pub fn step_in_place(cfg: &mut Config, stale: &mut BTreeSet<Id>, p: &Phrase) -> (Vec<Output>, bool) {
    let mut outs = Vec::new();
    let mut named = BTreeSet::new();
    phrase_mentions(p, &mut named);
    if named.contains("*") {
        if let Err(e) = freshen(cfg, stale, None) {
            outs.push(Output::Runtime(e));
            return (outs, false);
        }
        let (outs, next) = step_with(cfg, p, false);
        let Some(next) = next else { return (outs, false) };
        *cfg = next;
        *stale = lazy_types(&cfg.spec);
        return (outs, true);
    }
    let compiled = match ready(cfg, stale, p, named) {
        Ok(c) => c,
        Err(e) => {
            outs.extend(e);
            return (outs, false);
        }
    };
    let Some(store) = sem_phrase(&compiled, &cfg.spec, &cfg.state, &mut outs) else { return (outs, false) };
    if !store.is_empty() {
        assign(&cfg.spec, &store, &mut cfg.state);
        stale.extend(derived_types(&cfg.spec));
    }
    cfg.transitions.clear();
    (outs, true)
}

/// P compiled, and what it reads worked out in place: the stale derived types among the types it names and those their
/// rules read; one instance of a stale derived type it performs or asks about alone, alone. What `step_in_place` and
/// `transition_in_place` do before P itself.
pub(super) fn ready(cfg: &mut Config, stale: &mut BTreeSet<Id>, p: &Phrase, named: BTreeSet<Id>) -> Result<CPhrase, Vec<Output>> {
    let compiled = (Static { spec: &mut cfg.spec }).phrase(p).map_err(|errs| vec![Output::CompilationError(errs)])?;
    let mut read = reads(&cfg.spec, named);
    // one instance of a stale derived type performed or asked about alone: that instance alone is worked out
    let one = one_instance(cfg, stale, &compiled);
    if let Some(te) = &one {
        read.remove(&te.dom);
    }
    freshen(cfg, stale, Some(&read)).map_err(|e| vec![Output::Runtime(e)])?;
    // a type the rest reads was worked out whole, this one with it
    if let Some(te) = one.filter(|te| stale.contains(&te.dom)) {
        // an instance read with others left unknown is worked out with its whole type, by the well-founded model
        let found = match cfg.state.unknown().is_empty() {
            true => Ev::plain(&cfg.spec, &cfg.state).derives(&te),
            false => Ok(None),
        };
        let done = match found {
            Err(e) => Err(e),
            Ok(None) => freshen(cfg, stale, Some(&BTreeSet::from([te.dom.clone()]))),
            Ok(Some(holds)) => {
                if cfg.state.contents().get(&te).is_some_and(|i| i.derived) {
                    cfg.state.remove(&te);
                }
                if holds && !cfg.state.contents().contains_key(&te) {
                    cfg.state.insert(te, Info { value: true, derived: true });
                }
                Ok(())
            }
        };
        done.map_err(|e| vec![Output::Runtime(e)])?;
    }
    Ok(compiled)
}

/// The act or event P triggers worked out on CFG's state as it stands (its instance given whole, no variable), not
/// put in place: its transition, with whether it was enabled. What a caller that checks a step before keeping it reads.
pub fn transition_in_place(cfg: &mut Config, stale: &mut BTreeSet<Id>, p: &Phrase) -> Result<TransInfo, Vec<Output>> {
    transition_read(cfg, stale, p, false)
}

/// [`transition_in_place`] read on the upper bound: what it does and whether it is enabled once every unknown
/// instance is taken to hold, and to fail where read through `Not`.
pub fn transition_may(cfg: &mut Config, stale: &mut BTreeSet<Id>, p: &Phrase) -> Result<TransInfo, Vec<Output>> {
    transition_read(cfg, stale, p, true)
}

fn transition_read(cfg: &mut Config, stale: &mut BTreeSet<Id>, p: &Phrase, upper: bool) -> Result<TransInfo, Vec<Output>> {
    let mut named = BTreeSet::new();
    phrase_mentions(p, &mut named);
    let compiled = ready(cfg, stale, p, named)?;
    let CPhrase::Trigger(vs, t) = &compiled else { return Err(vec![Output::Runtime(RtErr::Internal("not a trigger".into()))]) };
    let mut ev = if upper { Ev::upper(&cfg.spec, &cfg.state) } else { Ev::new(&cfg.spec, &cfg.state) };
    let tes = ev.foreach(vs, &mut |ev| Ok(taggeds_rev(ev.eval(t)?))).map_err(|e| vec![Output::Runtime(e)])?;
    let [te] = &tes[..] else {
        return Err(vec![Output::Runtime(RtErr::Internal(format!("{} instances, one expected", tes.len())))]);
    };
    if !cfg.spec.triggerable(&te.dom) {
        return Err(vec![Output::NotTriggerable(te.dom.clone())]);
    }
    match ev.instantiate_trans(&BTreeSet::new(), te) {
        Ok(mut one) if one.len() == 1 => Ok(one.pop().expect("one")),
        Ok(_) => Err(vec![Output::Runtime(RtErr::Internal(format!("{} is not a single transition", te)))]),
        Err(e) => Err(vec![Output::Runtime(e)]),
    }
}

/// The one instance of a stale derived type a phrase performs (`act(..).`) or asks about (`?fact(..).`) alone, where
/// the type's own clauses (rules, conditions, effects, syncs, violations) read no instance of it, directly or through
/// other types: then nothing the phrase does reads its other instances.
pub(super) fn one_instance(cfg: &Config, stale: &BTreeSet<Id>, compiled: &CPhrase) -> Option<Tagged> {
    let t = match compiled {
        CPhrase::Trigger(vs, t) if vs.is_empty() => t,
        CPhrase::Query(Term::Un(UnOp::Present, t)) => &**t,
        _ => return None,
    };
    let tes = Ev::new(&cfg.spec, &cfg.state).eval(t).map(taggeds_rev).ok()?;
    let [te] = &tes[..] else { return None };
    let ty = cfg.spec.find(&te.dom)?;
    if !stale.contains(&te.dom) || reads(&cfg.spec, type_mentions(ty)).contains(&cfg.spec.chase(&te.dom)) {
        return None;
    }
    Some(te.clone())
}

/// Work out the stale derived types among READ (all of them if `None`), and the stale types their rules read, on
/// CFG's state in place; they are stale no more. On an error they stay stale, with none of their derived instances.
pub fn freshen(cfg: &mut Config, stale: &mut BTreeSet<Id>, read: Option<&BTreeSet<Id>>) -> R<()> {
    let todo: BTreeSet<Id> = match read {
        None => stale.clone(),
        Some(read) => reads(&cfg.spec, read.intersection(stale).cloned().collect()).intersection(stale).cloned().collect(),
    };
    if todo.is_empty() {
        return Ok(());
    }
    cfg.state.retain_underived_of(&todo);
    if let Err(e) = saturate_only_in(&cfg.spec, &mut cfg.state, &todo) {
        cfg.state.retain_underived_of(&todo);
        return Err(e);
    }
    stale.retain(|d| !todo.contains(d));
    Ok(())
}
