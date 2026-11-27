use super::*;
use crate::docket::{Bind, Docket, Texts};
use crate::peer::{Party, Vouch};
use crate::serve::Terminals;
use docket_v2::norm::Core;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

const TEXT: &str = r#"
module t
Fact Person Identified by String
Fact N(k: Int)
Act Put(by: Person, k: Int)
  Creates N(k)
+Person("w1").
"#;

struct Anyone;

impl Vouch for Anyone {
    fn party(&self, _: &str) -> Option<Party> {
        Some(Party::of("w1", None))
    }
}

struct NoTerminals;

impl Terminals for NoTerminals {
    fn open(&self, _: &Party, s: &str) -> Result<(mpsc::UnboundedSender<Vec<u8>>, broadcast::Receiver<Vec<u8>>), String> {
        Err(format!("no terminal {s}"))
    }
}

fn engine() -> Rc<Engine> {
    let dir = std::env::temp_dir().join(format!("subscribe-lag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let loaded = docket_v2::check::load(vec![docket_v2::syntax::read(TEXT).unwrap()]).unwrap();
    let runs = docket_v2::run::machine::Machine::new(loaded, Core::default(), vec![]).unwrap();
    let (bind, texts): (Bind, Texts) = (Box::new(|_, _| (None, String::new())), Box::new(|_| Err("this record starts empty".into())));
    Engine::open(&dir.join("record.jsonl"), Box::new(Docket::new(runs, "t", vec![], bind, texts)), Box::new(Anyone), Box::new(NoTerminals)).unwrap()
}

/// The view a `done` or a `reset` gives.
fn parsed(value: &Json) -> View {
    let lists = |k: &str| value[k].as_object().unwrap().iter().map(|(t, xs)| (t.clone(), crate::machine::held(xs.as_array().unwrap().iter().cloned()))).collect();
    View { facts: lists("facts"), duties: lists("duties"), violated: lists("violated") }
}

/// A subscriber to every type that falls further behind than the stream holds is sent what holds anew, and the frames
/// after it go on from there; its client ends as a whole read, and no view is kept for it meanwhile.
#[tokio::test]
async fn a_reader_too_far_behind_starts_again_from_what_holds() {
    LocalSet::new()
        .run_until(async {
            let engine = engine();
            let slot = Rc::new(Slot::default());
            let Reply::Done { value } = engine.subscribe(None, &slot) else { panic!("no done") };
            let (mut client, mut at) = (parsed(&value), value["seq"].as_u64().unwrap());
            let w1 = Party::of("w1", None);
            let put = |k: i64| match engine.change(|m| m.act(&w1, "t.Put", &[json!("w1"), json!(k)])) {
                Reply::Done { .. } => {}
                r => panic!("put {k}: {r:?}"),
            };
            // more changes than the stream holds, and no turn for the subscription between them
            (0..300).for_each(put);
            let (mut resets, mut frames) = (0, 0);
            for k in 300..320 {
                for _ in 0..5 {
                    tokio::task::yield_now().await;
                }
                while let Some(r) = slot.take() {
                    match r {
                        Reply::Reset { value } => {
                            (client, at, resets) = (parsed(&value), value["seq"].as_u64().unwrap(), resets + 1);
                        }
                        Reply::Delta { since, delta } => {
                            assert_eq!(since, at, "each frame goes on from the last one sent");
                            crate::delta::apply(&mut client, &delta);
                            (at, frames) = (delta.seq, frames + 1);
                        }
                        r => panic!("{r:?}"),
                    }
                }
                put(k);
            }
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            while let Some(Reply::Delta { delta, .. }) = slot.take() {
                crate::delta::apply(&mut client, &delta);
            }
            assert_eq!(resets, 1, "one reset for the 300 missed");
            assert!(frames >= 10, "frames went on after it: {frames}");
            let names: Vec<String> = client.facts.keys().chain(client.duties.keys()).cloned().collect();
            let whole = engine.core.borrow_mut().machine.view(Some(&names)).unwrap();
            assert_eq!(client, whole, "the client is a whole read");
            assert_eq!(client.facts["t.N"].len(), 320);
        })
        .await;
}
