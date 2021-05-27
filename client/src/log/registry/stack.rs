pub(crate) use tracing_core::span::Id;

#[derive(Debug)]
struct ContextId {
    id: Id,
    duplicate: bool,
}

/// `SpanStack` tracks what spans are currently executing on a thread-local basis.
///
/// A "separate current span" for each thread is a semantic choice, as each span
/// can be executing in a different thread.
#[derive(Debug, Default)]
pub(crate) struct SpanStack {
    stack: Vec<ContextId>,
}

impl SpanStack {
    #[inline]
    pub(crate) fn push(&mut self, id: Id) -> bool {
        let duplicate = self.stack.iter().any(|i| i.id == id);
        self.stack.push(ContextId { id, duplicate });
        !duplicate
    }

    #[inline]
    pub(crate) fn pop(&mut self, expected_id: &Id) -> bool {
        if let Some((idx, _)) = self
            .stack
            .iter()
            .enumerate()
            .rev()
            .find(|(_, ctx_id)| ctx_id.id == *expected_id)
        {
            let ContextId { id: _, duplicate } = self.stack.remove(idx);
            return !duplicate;
        }
        false
    }

    #[inline]
    pub(crate) fn current(&self) -> Option<&Id> {
        self.stack
            .iter()
            .rev()
            .find(|context_id| !context_id.duplicate)
            .map(|context_id| &context_id.id)
    }
}
