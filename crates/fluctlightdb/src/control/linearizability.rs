//! Small exhaustive checker for bounded control-plane and acknowledged-mutation histories.

#[derive(Debug, Clone)]
pub struct TimedOperation<O> {
    pub invoked_at: u64,
    pub completed_at: u64,
    pub operation: O,
}

impl<O> TimedOperation<O> {
    pub fn new(invoked_at: u64, completed_at: u64, operation: O) -> Self {
        Self {
            invoked_at,
            completed_at,
            operation,
        }
    }
}

pub fn check_linearizable<M, O, F>(initial: M, history: &[TimedOperation<O>], mut apply: F) -> bool
where
    M: Clone,
    F: FnMut(&mut M, &O) -> bool,
{
    if history
        .iter()
        .any(|operation| operation.completed_at < operation.invoked_at)
    {
        return false;
    }
    let mut remaining = vec![true; history.len()];
    search(initial, history, &mut remaining, &mut apply)
}

fn search<M, O, F>(
    model: M,
    history: &[TimedOperation<O>],
    remaining: &mut [bool],
    apply: &mut F,
) -> bool
where
    M: Clone,
    F: FnMut(&mut M, &O) -> bool,
{
    if remaining.iter().all(|pending| !pending) {
        return true;
    }
    for index in 0..history.len() {
        if !remaining[index] {
            continue;
        }
        let has_real_time_predecessor = history.iter().enumerate().any(|(other, candidate)| {
            other != index && remaining[other] && candidate.completed_at < history[index].invoked_at
        });
        if has_real_time_predecessor {
            continue;
        }

        let mut next = model.clone();
        if !apply(&mut next, &history[index].operation) {
            continue;
        }
        remaining[index] = false;
        if search(next, history, remaining, apply) {
            remaining[index] = true;
            return true;
        }
        remaining[index] = true;
    }
    false
}
