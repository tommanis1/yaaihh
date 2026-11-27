//! The state's indexes answer as a scan of every instance does.

use super::*;


/// Every instance of D in `contents`' order, by truth, by a scan of every instance: what the index must answer.
fn scanned(s: &State, d: &Id) -> (Vec<Tagged>, Vec<Tagged>) {
    let of: Vec<(Tagged, bool)> = s.contents.iter().filter(|(te, _)| te.dom == *d).map(|(te, i)| (te.clone(), i.value)).collect();
    (of.iter().filter(|x| x.1).map(|x| x.0.clone()).collect(), of.iter().filter(|x| !x.1).map(|x| x.0.clone()).collect())
}

/// The index answers as the scan does after inserts, overwrites, removals and the derived dropped,
/// over types whose instances interleave in `contents`' order.
#[test]
fn the_instances_by_type_are_the_scan_of_every_instance() {
    let (a, b) = (id("a"), id("b"));
    let t = |d: &Id, n: i64| Tagged { elem: Elem::Int(n), dom: d.clone() };
    let mut s = State::default();
    for n in [5, 1, 3, 2, 4] {
        s.insert(t(&a, n), Info { value: n % 2 == 1, derived: n == 3 });
        s.insert(t(&b, n), Info { value: n % 2 == 0, derived: n == 4 });
    }
    s.insert(t(&a, 2), Info { value: true, derived: false });
    s.remove(&t(&b, 1));
    s.remove(&t(&b, 9));
    for d in [&a, &b, &id("c")] {
        assert_eq!(View { base: &s, over: None }.assigned(d), scanned(&s, d));
    }
    s.retain_underived();
    assert!(!s.contents.contains_key(&t(&a, 3)) && !s.contents.contains_key(&t(&b, 4)));
    for d in [&a, &b] {
        assert_eq!(View { base: &s, over: None }.assigned(d), scanned(&s, d));
    }
    for n in [1, 2, 4, 5] {
        s.remove(&t(&a, n));
    }
    assert_eq!(View { base: &s, over: None }.assigned(&a), (vec![], vec![]));
    assert!(!s.by_type.contains_key(&a));
}

/// What `by_field` must hold: every instance of a composite type, stored or derived, under each of its fields' values.
fn fields_scanned(s: &State) -> BTreeMap<(Id, usize, Elem), BTreeSet<Elem>> {
    let mut out: BTreeMap<(Id, usize, Elem), BTreeSet<Elem>> = BTreeMap::new();
    for te in s.contents.keys() {
        if let Elem::Prod(args) = &te.elem {
            for (n, a) in args.iter().enumerate() {
                out.entry((te.dom.clone(), n, a.elem.clone())).or_default().insert(te.elem.clone());
            }
        }
    }
    out
}

/// The instances by field are the scan of every instance after inserts, a derived instance stored and a stored one
/// derived, removals and the derived dropped.
#[test]
fn the_instances_by_field_are_the_scan_of_every_instance() {
    let (p, q) = (id("p"), id("q"));
    let t = |d: &Id, a: i64, b: i64| {
        let n = |k: i64| Tagged { elem: Elem::Int(k), dom: id("n") };
        Tagged { elem: Elem::Prod(vec![n(a), n(b)].into()), dom: d.clone() }
    };
    let pairs = [(1, 2), (2, 1), (1, 1), (3, 2), (2, 2)];
    let mut s = State::default();
    for (a, b) in pairs {
        s.insert(t(&p, a, b), Info { value: a != 3, derived: a == b });
        s.insert(t(&q, b, a), Info { value: true, derived: false });
        assert_eq!(s.by_field, fields_scanned(&s));
    }
    s.insert(t(&p, 1, 1), Info { value: true, derived: false });
    s.insert(t(&p, 1, 2), Info { value: true, derived: true });
    assert_eq!(s.by_field, fields_scanned(&s));
    s.remove(&t(&q, 2, 1));
    s.remove(&t(&p, 9, 9));
    assert_eq!(s.by_field, fields_scanned(&s));
    s.retain_underived_of(&BTreeSet::from([p.clone()]));
    assert_eq!(s.by_field, fields_scanned(&s));
    s.retain_underived();
    assert_eq!(s.by_field, fields_scanned(&s));
    for (a, b) in pairs {
        s.remove(&t(&p, a, b));
        s.remove(&t(&q, b, a));
    }
    assert!(s.by_field.is_empty() && s.contents.is_empty());
}

/// What `values` must hold: every field of every instance that holds, stored or derived, counted by its value.
fn values_scanned(s: &State) -> BTreeMap<(Id, Elem), usize> {
    let mut out: BTreeMap<(Id, Elem), usize> = BTreeMap::new();
    for (te, i) in &s.contents {
        if let (Elem::Prod(args), true) = (&te.elem, i.value) {
            for a in args.iter() {
                *out.entry((a.dom.clone(), a.elem.clone())).or_default() += 1;
            }
        }
    }
    out
}

/// The values fields take are the scan of every instance that holds after inserts, a truth changed both ways, a
/// derived instance stored and a stored one derived, removals and the derived dropped; `occurs` and `values_of`
/// read them by type, in the order of values.
#[test]
fn the_values_fields_take_are_the_scan_of_every_instance_that_holds() {
    let (p, q, n) = (id("p"), id("q"), id("n"));
    let k = |v: i64| Tagged { elem: Elem::Int(v), dom: n.clone() };
    let t = |d: &Id, a: i64, b: i64| Tagged { elem: Elem::Prod(vec![k(a), k(b)].into()), dom: d.clone() };
    let pairs = [(1, 2), (2, 1), (1, 1), (3, 2), (2, 2)];
    let mut s = State::default();
    for (a, b) in pairs {
        s.insert(t(&p, a, b), Info { value: a != 3, derived: a == b });
        s.insert(t(&q, b, a), Info { value: true, derived: false });
        assert_eq!(s.values, values_scanned(&s));
    }
    s.insert(t(&p, 3, 2), Info { value: true, derived: false });
    s.insert(t(&p, 1, 2), Info { value: false, derived: true });
    assert_eq!(s.values, values_scanned(&s));
    s.remove(&t(&q, 2, 1));
    s.remove(&t(&p, 9, 9));
    assert_eq!(s.values, values_scanned(&s));
    s.retain_underived_of(&BTreeSet::from([p.clone()]));
    assert_eq!(s.values, values_scanned(&s));
    assert!(s.occurs(&k(3)) && !s.occurs(&k(9)));
    assert_eq!(s.values_of(&n).collect::<Vec<_>>(), [1, 2, 3].map(Elem::Int));
    s.retain_underived();
    assert_eq!(s.values, values_scanned(&s));
    for (a, b) in pairs {
        s.remove(&t(&p, a, b));
        s.remove(&t(&q, b, a));
    }
    assert!(s.values.is_empty() && s.values_of(&n).next().is_none());
}
