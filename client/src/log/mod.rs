//! The name of our subscriber is Chad because it's awesome.

use core::iter;
use std::any::TypeId;
use std::cell::RefCell;
use std::fmt;
use std::fmt::Write;
use std::marker::PhantomData;
use std::ops::Deref;

use ansi_term::Style;
use tracing::{Event, Id, Metadata};
use tracing::level_filters::LevelFilter;
use tracing::span::{Attributes, Record};


use tracing_core::span::Current;
use tracing_log::NormalizeEvent;
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::{FormatFields};
use tracing_subscriber::fmt::format::DefaultFields;
use tracing_subscriber::fmt::time::FormatTime;


use registry::sharded::Registry;
use shared::dep::{serde_cbor, tokio};


use crate::error::{Error, Result};
use crate::log::protocol::MessageToLogProcess;
use crate::log::registry::{LookupSpan, SpanRef, FromRoot};
use shared::dep::tokio::{sync::mpsc::UnboundedSender};
use shared::dep::tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
use shared::dep::futures::SinkExt;
use crate::log::writer::{AsyncExtraTerminalWriter};


mod writer;
pub mod protocol;
mod registry;


/// Provides the current span context to a formatter.
pub struct FmtContext<'a, N> {
    pub(crate) ctx: Context<'a, Registry>,
    pub(crate) fmt_fields: &'a N,
}

impl<'a, N> fmt::Debug for FmtContext<'a, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FmtContext").finish()
    }
}

impl<'cx, 'writer, N> FormatFields<'writer> for FmtContext<'cx, N>
    where
        N: FormatFields<'writer> + 'static,
{
    fn format_fields<R: RecordFields>(
        &self,
        writer: &'writer mut dyn fmt::Write,
        fields: R,
    ) -> fmt::Result {
        self.fmt_fields.format_fields(writer, fields)
    }
}

impl<'a, N> FmtContext<'a, N>
    where
        N: for<'writer> FormatFields<'writer> + 'static,
{
    /// Visits every span in the current context with a closure.
    ///
    /// The provided closure will be called first with the current span,
    /// and then with that span's parent, and then that span's parent,
    /// and so on until a root span is reached.
    pub fn visit_spans<E, F>(&self, mut f: F) -> std::result::Result<(), E>
        where
            F: FnMut(&SpanRef<'_, Registry>) -> std::result::Result<(), E>,
    {
        // visit all the current spans
        for span in self.ctx.scope() {
            f(&span)?;
        }
        Ok(())
    }

    /// Returns metadata for the span with the given `id`, if it exists.
    ///
    /// If this returns `None`, then no span exists for that ID (either it has
    /// closed or the ID is invalid).
    #[inline]
    pub fn metadata(&self, id: &Id) -> Option<&'static Metadata<'static>>
    {
        self.ctx.metadata(id)
    }

    /// Returns [stored data] for the span with the given `id`, if it exists.
    ///
    /// If this returns `None`, then no span exists for that ID (either it has
    /// closed or the ID is invalid).
    ///
    /// [stored data]: ../registry/struct.SpanRef.html
    #[inline]
    pub fn span(&self, id: &Id) -> Option<SpanRef<'_, Registry>>
    {
        self.ctx.span(id)
    }

    /// Returns `true` if an active span exists for the given `Id`.
    #[inline]
    pub fn exists(&self, id: &Id) -> bool
    {
        self.ctx.exists(id)
    }

    /// Returns [stored data] for the span that the wrapped subscriber considers
    /// to be the current.
    ///
    /// If this returns `None`, then we are not currently within a span.
    ///
    /// [stored data]: ../registry/struct.SpanRef.html
    #[inline]
    pub fn lookup_current(&self) -> Option<SpanRef<'_, Registry>>
    {
        self.ctx.lookup_current()
    }

    /// Returns an iterator over the [stored data] for all the spans in the
    /// current context, starting the root of the trace tree and ending with
    /// the current span.
    ///
    /// [stored data]: ../registry/struct.SpanRef.html
    pub fn scope(&self) -> Scope<'_, Registry>
    {
        self.ctx.scope()
    }

    /// Returns the current span for this formatter.
    pub fn current_span(&self) -> Current {
        self.ctx.current_span()
    }

    /// Returns the [field formatter] configured by the subscriber invoking
    /// `format_event`.
    ///
    /// The event formatter may use the returned field formatter to format the
    /// fields of any events it records.
    ///
    /// [field formatter]: FormatFields
    pub fn field_format(&self) -> &N {
        self.fmt_fields
    }
}


/// He helps formats the events
#[derive(Default)]
struct HolySpirit;

impl HolySpirit {
    #[inline]
    fn format_timestamp(&self, writer: &mut dyn fmt::Write) -> fmt::Result
    {
        let style = Style::new().dimmed();
        write!(writer, "{}", style.prefix())?;
        tracing_subscriber::fmt::time::SystemTime.format_time(writer)?;
        write!(writer, "{} ", style.suffix())?;
        return Ok(());
    }
}


struct FmtThreadName<'a> {
    name: &'a str,
}

impl<'a> FmtThreadName<'a> {
    pub(crate) fn new(name: &'a str) -> Self {
        Self { name }
    }
}

impl<'a> fmt::Display for FmtThreadName<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::sync::atomic::{
            AtomicUsize,
            Ordering::{AcqRel, Acquire, Relaxed},
        };

        // Track the longest thread name length we've seen so far in an atomic,
        // so that it can be updated by any thread.
        static MAX_LEN: AtomicUsize = AtomicUsize::new(0);
        let len = self.name.len();
        // Snapshot the current max thread name length.
        let mut max_len = MAX_LEN.load(Relaxed);

        while len > max_len {
            // Try to set a new max length, if it is still the value we took a
            // snapshot of.
            match MAX_LEN.compare_exchange(max_len, len, AcqRel, Acquire) {
                // We successfully set the new max value
                Ok(_) => break,
                // Another thread set a new max value since we last observed
                // it! It's possible that the new length is actually longer than
                // ours, so we'll loop again and check whether our length is
                // still the longest. If not, we'll just use the newer value.
                Err(actual) => max_len = actual,
            }
        }

        // pad thread name using `max_len`
        write!(f, "{:>width$}", self.name, width = max_len)
    }
}


struct FullCtx<'a, N>
    where
        N: for<'writer> FormatFields<'writer> + 'static,
{
    ctx: &'a FmtContext<'a, N>,
    span: Option<&'a tracing_core::span::Id>,
}

impl<'a, N: 'a> FullCtx<'a, N>
    where
        N: for<'writer> FormatFields<'writer> + 'static,
{
    pub(crate) fn new(
        ctx: &'a FmtContext<'a, N>,
        span: Option<&'a tracing_core::span::Id>,
    ) -> Self {
        Self { ctx, span }
    }

    fn bold(&self) -> Style {
        return Style::new().bold();
    }
}

impl<'a, N> fmt::Display for FullCtx<'a, N>
    where
        N: for<'writer> FormatFields<'writer> + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bold = self.bold();
        let mut seen = false;

        let span = self
            .span
            .and_then(|id| self.ctx.ctx.span(&id))
            .or_else(|| self.ctx.ctx.lookup_current());

        let scope = span
            .into_iter()
            .flat_map(|span| span.ref_from_root().chain(iter::once(span)));

        for span in scope {
            write!(f, "{}", bold.paint(span.metadata().name()))?;
            seen = true;

            let ext = span.extensions();
            let fields = &ext
                .get::<FormattedFields<N>>()
                .expect("Unable to find FormattedFields in extensions; this is a bug");
            if !fields.is_empty() {
                write!(f, "{}{}{}", bold.paint("{"), fields, bold.paint("}"))?;
            }
            f.write_char(':')?;
        }

        if seen {
            f.write_char(' ')?;
        }
        Ok(())
    }
}


impl HolySpirit
{
    fn format_event<FF: for<'a> FormatFields<'a> + 'static>(&self, ctx: &FmtContext<'_, FF>, writer: &mut dyn Write, event: &Event<'_>) -> Result<protocol::Level>
    {
        let normalized_meta = event.normalized_metadata();
        let meta = normalized_meta.as_ref().unwrap_or_else(|| event.metadata());

        self.format_timestamp(writer).map_err(|e| Error::FormatTimestamp(e))?;


        let current_thread = std::thread::current();
        match current_thread.name() {
            Some(name) => {
                write!(writer, "{} ", FmtThreadName::new(name)).map_err(|e| Error::FormatThreadName(e))?;
            }
            _ => {}
        }


        let full_ctx = {
            {
                FullCtx::new(ctx, event.parent())
            }
        };

        write!(writer, "{}", full_ctx).map_err(|e| Error::FormatFullContext(e))?;


        write!(writer, "{}: ", meta.target()).map_err(|e| Error::FormatTarget(e))?;

        ctx.format_fields(writer, event).map_err(|e| Error::FormatFields(e))?;
        writeln!(writer).unwrap();

        Ok(protocol::Level::from(*meta.level()))
    }
}


pub struct Chad {
    registry: Registry,
    /// tells chad how to format fields
    fmt_fields: DefaultFields,
    /// tells chad how to format events
    fmt_event: HolySpirit,
    message_tx: UnboundedSender<MessageToLogProcess>,
}


impl Chad {
    fn new(message_tx: UnboundedSender<MessageToLogProcess>) -> Self {
        Self {
            registry: Registry::default(),
            fmt_fields: DefaultFields::default(),
            fmt_event: HolySpirit::default(),
            message_tx,
        }
    }


    fn ctx(&self) -> Context<'_, Registry> {
        Context {
            subscriber: Some(&self.registry),
        }
    }

    #[inline]
    fn make_ctx<'a>(&'a self, ctx: Context<'a, Registry>) -> FmtContext<'a, DefaultFields> {
        FmtContext {
            ctx,
            fmt_fields: &self.fmt_fields,
        }
    }
}

impl tracing_core::Subscriber for Chad {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        let _ = metadata;
        true
    }

    fn max_level_hint(&self) -> Option<LevelFilter> {
        Some(LevelFilter::TRACE)
    }

    fn new_span(&self, span: &Attributes<'_>) -> Id {
        self.registry.new_span(span)
    }

    fn record(&self, span: &Id, values: &Record<'_>) {
        let span = self.registry.span(span).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();
        if let Some(FormattedFields { ref mut fields, .. }) =
        extensions.get_mut::<FormattedFields<DefaultFields>>()
        {
            let _ = self.fmt_fields.add_fields(fields, values);
        } else {
            let mut buf = String::new();
            if self.fmt_fields.format_fields(&mut buf, values).is_ok() {
                let fmt_fields = FormattedFields {
                    fields: buf,
                    _format_event: PhantomData::<fn(DefaultFields)>,
                };
                extensions.insert(fmt_fields);
            }
        }
    }

    fn record_follows_from(&self, span: &Id, follows: &Id) {
        let _ = (span, follows);
    }


    fn event(&self, event: &Event<'_>) {
        thread_local! {
            static BUF: RefCell<String> = RefCell::new(String::new());
        }

        BUF.with(|buf| {
            let borrow = buf.try_borrow_mut();
            let mut a;
            let mut b;
            let mut buf = match borrow {
                Ok(buf) => {
                    a = buf;
                    &mut *a
                }
                _ => {
                    b = String::new();
                    &mut b
                }
            };
            let ctx = self.make_ctx(self.ctx());
            if let Ok(event_level) = self.fmt_event.format_event(&ctx, &mut buf, event) {
                let message = MessageToLogProcess { level: event_level, formatted_event: buf.to_string() };
                if let Err(_) = self.message_tx.send(message) {};
            }

            buf.clear();
        });
    }

    fn enter(&self, span: &Id) {
        self.registry.enter(span);
    }

    fn exit(&self, span: &Id) {
        self.registry.exit(span);
    }

    fn clone_span(&self, id: &Id) -> Id {
        self.registry.clone_span(id)
    }


    fn try_close(&self, id: Id) -> bool {
        let subscriber = &self.registry as &dyn tracing_core::Subscriber;

        let mut guard = subscriber
            .downcast_ref::<Registry>()
            .map(|registry| registry.start_close(id.clone()));

        if self.registry.try_close(id.clone()) {
            // If we have a registry's close guard, indicate that the span is
            // closing.

            {
                if let Some(g) = guard.as_mut() {
                    g.mut_is_closing()
                };
            }

            true
        } else {
            false
        }
    }


    fn current_span(&self) -> Current {
        self.registry.current_span()
    }

    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        if id == TypeId::of::<Self>() {
            return Some(self as *const _ as *const ());
        }
        self.registry.downcast_raw(id)
    }
}

/// A formatted representation of a span's fields stored in its [extensions].
///
/// Because `FormattedFields` is generic over the type of the formatter that
/// produced it, multiple versions of a span's formatted fields can be stored in
/// the [`Extensions`][extensions] type-map. This means that when multiple
/// formatters are in use, each can store its own formatted representation
/// without conflicting.
///
/// [extensions]: ../registry/struct.Extensions.html
#[derive(Default)]
pub struct FormattedFields<E> {
    _format_event: PhantomData<fn(E)>,
    /// The formatted fields of a span.
    pub fields: String,
}

impl<E> FormattedFields<E> {
    /// Returns a new `FormattedFields`.
    pub fn new(fields: String) -> Self {
        Self {
            fields,
            _format_event: PhantomData,
        }
    }
}

impl<E> fmt::Debug for FormattedFields<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FormattedFields")
            .field("fields", &self.fields)
            .finish()
    }
}

impl<E> fmt::Display for FormattedFields<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.fields)
    }
}

impl<E> Deref for FormattedFields<E> {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.fields
    }
}


#[derive(Debug)]
pub struct Context<'a, S> {
    subscriber: Option<&'a S>,
}


impl<'a, S> Context<'a, S>
    where
        S: tracing_core::Subscriber,
{
    /// Returns the wrapped subscriber's view of the current span.
    #[inline]
    pub fn current_span(&self) -> tracing_core::span::Current {
        self.subscriber
            .map(tracing_core::Subscriber::current_span)
            // TODO: this would be more correct as "unknown", so perhaps
            // `tracing-core` should make `Current::unknown()` public?
            .unwrap_or_else(tracing_core::span::Current::none)
    }

    /// Returns whether the wrapped subscriber would enable the current span.
    #[inline]
    pub fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.subscriber
            .map(|subscriber| subscriber.enabled(metadata))
            // If this context is `None`, we are registering a callsite, so
            // return `true` so that the layer does not incorrectly assume that
            // the inner subscriber has disabled this metadata.
            // TODO(eliza): would it be more correct for this to return an `Option`?
            .unwrap_or(true)
    }

    /// Records the provided `event` with the wrapped subscriber.
    ///
    /// # Notes
    ///
    /// - The subscriber is free to expect that the event's callsite has been
    ///   [registered][register], and may panic or fail to observe the event if this is
    ///   not the case. The `tracing` crate's macros ensure that all events are
    ///   registered, but if the event is constructed through other means, the
    ///   user is responsible for ensuring that [`register_callsite`][register]
    ///   has been called prior to calling this method.
    /// - This does _not_ call [`enabled`] on the inner subscriber. If the
    ///   caller wishes to apply the wrapped subscriber's filter before choosing
    ///   whether to record the event, it may first call [`Context::enabled`] to
    ///   check whether the event would be enabled. This allows `Layer`s to
    ///   elide constructing the event if it would not be recorded.
    ///
    /// [register]: https://docs.rs/tracing-core/latest/tracing_core/subscriber/trait.Subscriber.html#method.register_callsite
    /// [`enabled`]: https://docs.rs/tracing-core/latest/tracing_core/subscriber/trait.Subscriber.html#method.enabled
    /// [`Context::enabled`]: #method.enabled
    #[inline]
    pub fn event(&self, event: &Event<'_>) {
        if let Some(ref subscriber) = self.subscriber {
            subscriber.event(event);
        }
    }

    /// Returns metadata for the span with the given `id`, if it exists.
    ///
    /// If this returns `None`, then no span exists for that ID (either it has
    /// closed or the ID is invalid).
    #[inline]
    pub fn metadata(&self, id: &tracing_core::span::Id) -> Option<&'static Metadata<'static>>
        where
            S: for<'lookup> LookupSpan<'lookup>,
    {
        let span = self.subscriber.as_ref()?.span(id)?;
        Some(span.metadata())
    }

    /// Returns [stored data] for the span with the given `id`, if it exists.
    ///
    /// If this returns `None`, then no span exists for that ID (either it has
    /// closed or the ID is invalid).
    ///
    /// <div class="information">
    ///     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
    /// </div>
    /// <div class="example-wrap" style="display:inline-block">
    /// <pre class="ignore" style="white-space:normal;font:inherit;">
    /// <strong>Note</strong>: This requires the wrapped subscriber to implement the
    /// <a href="../registry/trait.LookupSpan.html"><code>LookupSpan</code></a> trait.
    /// See the documentation on <a href="./struct.Context.html"><code>Context</code>'s
    /// declaration</a> for details.
    /// </pre></div>
    ///
    /// [stored data]: ../registry/struct.SpanRef.html
    #[inline]
    pub fn span(&self, id: &tracing_core::span::Id) -> Option<SpanRef<'_, S>>
        where
            S: for<'lookup> LookupSpan<'lookup>,
    {
        self.subscriber.as_ref()?.span(id)
    }

    /// Returns `true` if an active span exists for the given `Id`.
    ///
    /// <div class="information">
    ///     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
    /// </div>
    /// <div class="example-wrap" style="display:inline-block">
    /// <pre class="ignore" style="white-space:normal;font:inherit;">
    /// <strong>Note</strong>: This requires the wrapped subscriber to implement the
    /// <a href="../registry/trait.LookupSpan.html"><code>LookupSpan</code></a> trait.
    /// See the documentation on <a href="./struct.Context.html"><code>Context</code>'s
    /// declaration</a> for details.
    /// </pre></div>
    #[inline]
    pub fn exists(&self, id: &tracing_core::span::Id) -> bool
        where
            S: for<'lookup> LookupSpan<'lookup>,
    {
        self.subscriber.as_ref().and_then(|s| s.span(id)).is_some()
    }

    /// Returns [stored data] for the span that the wrapped subscriber considers
    /// to be the current.
    ///
    /// If this returns `None`, then we are not currently within a span.
    ///
    /// <div class="information">
    ///     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
    /// </div>
    /// <div class="example-wrap" style="display:inline-block">
    /// <pre class="ignore" style="white-space:normal;font:inherit;">
    /// <strong>Note</strong>: This requires the wrapped subscriber to implement the
    /// <a href="../registry/trait.LookupSpan.html"><code>LookupSpan</code></a> trait.
    /// See the documentation on <a href="./struct.Context.html"><code>Context</code>'s
    /// declaration</a> for details.
    /// </pre></div>
    ///
    /// [stored data]: ../registry/struct.SpanRef.html
    #[inline]
    pub fn lookup_current(&self) -> Option<SpanRef<'_, S>>
        where
            S: for<'lookup> LookupSpan<'lookup>,
    {
        let subscriber = self.subscriber.as_ref()?;
        let current = subscriber.current_span();
        let id = current.id()?;
        let span = subscriber.span(&id);
        debug_assert!(
            span.is_some(),
            "the subscriber should have data for the current span ({:?})!",
            id,
        );
        span
    }

    /// Returns an iterator over the [stored data] for all the spans in the
    /// current context, starting the root of the trace tree and ending with
    /// the current span.
    ///
    /// If this iterator is empty, then there are no spans in the current context.
    ///
    /// <div class="information">
    ///     <div class="tooltip ignore" style="">ⓘ<span class="tooltiptext">Note</span></div>
    /// </div>
    /// <div class="example-wrap" style="display:inline-block">
    /// <pre class="ignore" style="white-space:normal;font:inherit;">
    /// <strong>Note</strong>: This requires the wrapped subscriber to implement the
    /// <a href="../registry/trait.LookupSpan.html"><code>LookupSpan</code></a> trait.
    /// See the documentation on <a href="./struct.Context.html"><code>Context</code>'s
    /// declaration</a> for details.
    /// </pre></div>
    ///
    /// [stored data]: ../registry/struct.SpanRef.html
    pub fn scope(&self) -> Scope<'_, S>
        where
            S: for<'lookup> LookupSpan<'lookup>,
    {
        let scope = self.lookup_current().map(|span| {
            let parents = span.ref_from_root();
            parents.chain(std::iter::once(span))
        });
        Scope(scope)
    }
}


impl<'a, S> Clone for Context<'a, S> {
    #[inline]
    fn clone(&self) -> Self {
        let subscriber = if let Some(ref subscriber) = self.subscriber {
            Some(*subscriber)
        } else {
            None
        };
        Context { subscriber }
    }
}

/// An iterator over the [stored data] for all the spans in the
/// current context, starting the root of the trace tree and ending with
/// the current span.
///
/// This is returned by [`Context::scope`].
///
/// [stored data]: ../registry/struct.SpanRef.html
/// [`Context::scope`]: struct.Context.html#method.scope
pub struct Scope<'a, L: LookupSpan<'a>>(
    Option<std::iter::Chain<FromRoot<'a, L>, std::iter::Once<SpanRef<'a, L>>>>,
);


impl<'a, L: LookupSpan<'a>> Iterator for Scope<'a, L> {
    type Item = SpanRef<'a, L>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut()?.next()
    }
}


impl<'a, L: LookupSpan<'a>> std::fmt::Debug for Scope<'a, L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad("Scope { .. }")
    }
}


pub async fn start_logging(main_end_rx: tokio::sync::oneshot::Receiver<()>) {
    let (tx,
        mut rx) = tokio::sync::mpsc::unbounded_channel::<MessageToLogProcess>();
    let subscriber = Chad::new(tx);
    tracing::subscriber::set_global_default(subscriber).unwrap();


    tokio::select! {
        _ = main_end_rx => {}
        _ = async move {
        let mut framed_write
            = FramedWrite::new(AsyncExtraTerminalWriter::default(),
                               LengthDelimitedCodec::new());
            while let Some(x) = rx.recv().await {
                framed_write.send(serde_cbor::to_vec(&x).unwrap().into()).await.unwrap();
            };
        } => {}
    }
}

#[macro_export]
macro_rules! finally {
    ($($first:ident)? $(::$part:ident)*! ($($args:tt)*), $fut:tt) => {
        async { defer! {$($first)?$(::$part)*!($($args)*);} $fut.await  }
    };
    ($($first:ident)? $(::$part:ident)*! ($($args:tt)*), $($second:ident)? $(::$second_part:ident)* ($($fut_args:tt)*)) => {
        async { defer! {$($first)?$(::$part)*!($($args)*);} $($second)?$(::$second_part)*($($fut_args)*).await  }
    }
}

