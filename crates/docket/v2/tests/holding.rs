//! The instances of a Fact or Duty that hold now, as the frame lists them (`Machine::holding`).

use docket_v2::check::load;
use docket_v2::norm::Core;
use docket_v2::run::machine::Machine;
use docket_v2::run::norms::Norms;
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::read;

const LENDING: &str = r#"
module t
Fact Person Identified by String
Fact Book Identified by String
Fact Lent(book: Book, to: Person, by: Person)
Duty Give-Back(holder: Person, claimant: Person, book: Book)
  Holds when Lent(book, holder, claimant)
Act Lend(by: Person, book: Book, to: Person)
  Creates Lent(book, to, by)
+Person("ann"). +Person("bob"). +Person("cy").
"#;

fn inst(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("t.{ty}"), args: args.iter().map(|a| Value::text(a)).collect() }
}

/// A Fact or Duty lists what holds in the order of values, stated, made by an act or derived; an Act or a name of
/// nothing is refused; the list leaves no read for the next run.
#[test]
fn a_fact_or_duty_lists_what_holds_in_the_order_of_values() {
    let mut m = Machine::new(load(vec![read(LENDING).unwrap()]).unwrap(), Core::default(), vec![]).unwrap();
    for (by, book, to) in [("cy", "b2", "ann"), ("ann", "b1", "bob")] {
        let s = m.norms.step(&inst("Lend", &[by, book, to]), Some(&Value::text(by))).unwrap();
        m.norms.commit(s);
    }
    m.norms.reads();
    assert_eq!(m.holding("t.Person").unwrap(), [inst("Person", &["ann"]), inst("Person", &["bob"]), inst("Person", &["cy"])]);
    assert_eq!(m.holding("t.Lent").unwrap(), [inst("Lent", &["b1", "bob", "ann"]), inst("Lent", &["b2", "ann", "cy"])]);
    assert_eq!(m.holding("t.Give-Back").unwrap(), [inst("Give-Back", &["ann", "cy", "b2"]), inst("Give-Back", &["bob", "ann", "b1"])]);
    assert_eq!(m.norms.reads(), [0; 32], "no read is left for a run");
    assert!(m.holding("t.Lend").unwrap_err().contains("no Fact or Duty"));
    assert!(m.holding("t.Shelf").is_err());
}
