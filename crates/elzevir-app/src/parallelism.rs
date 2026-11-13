// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure work split over threads, with results in input order
//! (docs/arquitetura/03-estrategia-reconstrucao.md, §1.2 item 8).
//!
//! Every batch job (import, indexing) has a serial switch, `ELZEVIR_SERIAL`, so that a
//! test can run the same batch both ways and compare the outcome.

use std::num::NonZeroUsize;

/// How a batch job runs its pure part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parallelism {
    /// On the calling thread.
    Serial,
    /// On this many threads at once.
    Threads(NonZeroUsize),
}

impl Parallelism {
    /// Serial when the `ELZEVIR_SERIAL` switch (`import,index`, any order, any case) names
    /// `job`; otherwise one thread per core. The caller reads the variable: this layer
    /// does not touch the environment.
    #[must_use]
    pub fn from_serial_switch(switch: Option<&str>, job: &str, cores: NonZeroUsize) -> Self {
        let serial = switch.is_some_and(|value| {
            value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(job))
        });
        if serial {
            Self::Serial
        } else {
            Self::Threads(cores)
        }
    }

    /// Applies `work` to every input and returns the outputs in input order. With threads,
    /// the inputs are cut into contiguous slices, one per thread, joined back in order: the
    /// result is the serial result, computed faster.
    pub fn map<T, U, F>(self, inputs: Vec<T>, work: F) -> Vec<U>
    where
        T: Send,
        U: Send,
        F: Fn(T) -> U + Sync,
    {
        let threads = match self {
            Self::Threads(threads) if threads.get() > 1 && inputs.len() > 1 => threads.get(),
            _ => return inputs.into_iter().map(work).collect(),
        };
        let per_thread = inputs.len().div_ceil(threads);
        let mut parts: Vec<Vec<T>> = Vec::with_capacity(threads);
        let mut rest = inputs.into_iter();
        loop {
            let part: Vec<T> = rest.by_ref().take(per_thread).collect();
            if part.is_empty() {
                break;
            }
            parts.push(part);
        }
        let work = &work;
        std::thread::scope(|scope| {
            let handles: Vec<_> = parts
                .into_iter()
                .map(|part| scope.spawn(move || part.into_iter().map(work).collect::<Vec<_>>()))
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| match handle.join() {
                    Ok(outputs) => outputs,
                    // The work is total (no panics in production code); a panic here is a
                    // bug, so it is surfaced instead of hidden.
                    Err(payload) => std::panic::resume_unwind(payload),
                })
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cores(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).unwrap_or(NonZeroUsize::MIN)
    }

    #[test]
    fn the_serial_switch_names_the_job() {
        let switch = Parallelism::from_serial_switch;
        assert_eq!(
            switch(None, "import", cores(8)),
            Parallelism::Threads(cores(8))
        );
        assert_eq!(
            switch(Some("import"), "import", cores(8)),
            Parallelism::Serial
        );
        assert_eq!(
            switch(Some("index, IMPORT"), "import", cores(8)),
            Parallelism::Serial
        );
        assert_eq!(
            switch(Some("index"), "import", cores(8)),
            Parallelism::Threads(cores(8))
        );
        assert_eq!(
            switch(Some("import,index"), "index", cores(8)),
            Parallelism::Serial
        );
    }

    #[test]
    fn threads_keep_the_input_order() {
        let inputs: Vec<u32> = (0..103).collect();
        let serial = Parallelism::Serial.map(inputs.clone(), |n| n * 2);
        for threads in [1, 2, 7, 200] {
            assert_eq!(
                Parallelism::Threads(cores(threads)).map(inputs.clone(), |n| n * 2),
                serial
            );
        }
        assert_eq!(
            Parallelism::Threads(cores(4)).map(Vec::<u32>::new(), |n| n),
            []
        );
    }
}
