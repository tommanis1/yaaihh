//! A job's setting kept between calls: each call runs over the tree the last successful one left in `out/`.

use crate::{ComputeDriver, JobResult, JobSpec, Stream};
use std::sync::atomic::AtomicBool;

pub struct WarmLayerHandle<D: ComputeDriver> {
    driver: D,
    base: JobSpec,
}

impl<D: ComputeDriver> WarmLayerHandle<D> {
    /// Calls run as `base` does, with their own command.
    pub fn new(driver: D, base: JobSpec) -> Self {
        WarmLayerHandle { driver, base }
    }

    pub fn call(&mut self, cmd: &[String], out: &mut dyn FnMut(Stream, &[u8])) -> Result<JobResult, String> {
        let job = JobSpec { cmd: cmd.to_vec(), ..self.base.clone() };
        let r = self.driver.run(&job, out, &AtomicBool::new(false))?;
        if let (0, Some(t)) = (r.exit_code, &r.output_tree_digest) {
            self.base.input_tree_digest = t.clone();
        }
        Ok(r)
    }

    pub fn driver(&mut self) -> &mut D {
        &mut self.driver
    }
}
