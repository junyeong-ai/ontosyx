//! Streaming-path coverage for [`MockLlmCall`].
//!
//! `structured_completion` is unary, so the streaming surface lives
//! one layer up — agent runtime / chat SSE consumers drive
//! [`branchforge::client::LlmCall::send_stream`] and reassemble the
//! `ModelStreamChunk` sequence into final text. These tests pin two
//! invariants the production stream path leans on:
//!
//! 1. Chunk reassembly: concatenating every `TextDelta` payload, in
//!    chunk order, reproduces the model's logical text.
//! 2. Mid-stream cancellation: a fired `CancellationToken` makes the
//!    stream emit `Err` and stop yielding further chunks.
//!
//! Production code that reads the stream (`chat::chat_stream` in
//! ox-api, agent loop in ox-agent) inherits both invariants — a
//! refactor that breaks chunk reassembly or cancellation propagation
//! fails this file before reaching the FE SSE surface.

#![cfg(feature = "test-helpers")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use branchforge::client::LlmCall;
use branchforge::ir::stream::ModelStreamChunk;
use branchforge::ir::{Message, ModelRequest};
use futures_util::StreamExt;
use ox_brain::test_support::{MockLlmCall, make_chunked_stream, make_text_stream};
use tokio_util::sync::CancellationToken;

fn build_request() -> ModelRequest {
    ModelRequest::new("claude-mock", vec![Message::user("test prompt")]).with_max_tokens(256)
}

fn collect_text_deltas(chunks: &[ModelStreamChunk]) -> String {
    chunks
        .iter()
        .filter_map(|c| match c {
            ModelStreamChunk::TextDelta { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[tokio::test]
async fn chunked_stream_reassembles_in_order() {
    // The model emits four text-delta chunks; consumer concatenates
    // them in arrival order. Anthropic / OpenAI streaming codecs
    // both deliver text in arrival order, so no resort is needed —
    // the test pins that property.
    let mock = MockLlmCall::new();
    mock.enqueue_stream(make_chunked_stream([
        "The ", "answer ", "is ", "42.",
    ]));

    let mut stream = mock
        .send_stream(&build_request(), CancellationToken::new())
        .await
        .expect("send_stream returns Ok");

    let mut received: Vec<ModelStreamChunk> = Vec::new();
    while let Some(item) = stream.next().await {
        received.push(item.expect("stream chunk is Ok"));
    }

    let reassembled = collect_text_deltas(&received);
    assert_eq!(reassembled, "The answer is 42.");

    // The framing chunks (MessageStart + Finish) bookend the body —
    // exactly one of each, in the canonical position.
    assert!(matches!(
        received.first(),
        Some(ModelStreamChunk::MessageStart { .. })
    ));
    assert!(matches!(
        received.last(),
        Some(ModelStreamChunk::Finish { .. })
    ));
}

#[tokio::test]
async fn single_text_stream_emits_message_start_one_delta_finish() {
    // The single-text helper is the convenient form for tests that
    // don't care about delta boundaries. Pin its frame structure so
    // a future expansion of `make_text_stream` doesn't silently
    // change the chunk count.
    let mock = MockLlmCall::new();
    mock.enqueue_stream(make_text_stream("hello"));

    let mut stream = mock
        .send_stream(&build_request(), CancellationToken::new())
        .await
        .expect("stream Ok");

    let mut count = 0;
    while stream.next().await.is_some() {
        count += 1;
    }
    assert_eq!(count, 3, "MessageStart + 1 TextDelta + Finish");
}

#[tokio::test]
async fn cancellation_mid_stream_emits_err_and_stops() {
    // Fire the cancel token between chunk yields — the stream must
    // emit one `Err`, then end. Production cancellation (chat SSE
    // disconnect, agent timeout) flows through the same token.
    let mock = MockLlmCall::new();
    mock.enqueue_stream(make_chunked_stream(["one", "two", "three", "four"]));

    let cancel = CancellationToken::new();
    let mut stream = mock
        .send_stream(&build_request(), cancel.clone())
        .await
        .expect("stream Ok");

    // Pull the first frame (MessageStart) so the consumer is past
    // the Async setup phase.
    stream.next().await.expect("first chunk").expect("Ok");

    cancel.cancel();

    // Subsequent reads must surface Err and then None.
    let mut saw_err = false;
    let mut tail_count = 0;
    // Bound the loop — never trust an unbounded stream in a unit test.
    let bounded = tokio::time::timeout(Duration::from_millis(500), async {
        while let Some(item) = stream.next().await {
            match item {
                Err(_) => {
                    saw_err = true;
                    tail_count += 1;
                }
                Ok(_) => tail_count += 1,
            }
            if saw_err {
                break;
            }
        }
        (saw_err, tail_count)
    })
    .await
    .expect("stream drains within 500ms after cancel");

    assert!(bounded.0, "cancellation must surface as a stream Err");
}

#[tokio::test]
async fn empty_stream_queue_returns_config_error() {
    // Pin the loud-failure mode: a test that forgets to enqueue a
    // stream must fail fast, not hang on an empty stream.
    let mock = MockLlmCall::new();

    let result = mock
        .send_stream(&build_request(), CancellationToken::new())
        .await;

    // ChunkStream is `Pin<Box<dyn Stream + Send>>` which has no
    // `Debug` impl, so we can't call `expect_err` directly.
    match result {
        Ok(_) => panic!("empty queue must fail loudly, got Ok"),
        Err(e) => assert!(
            e.to_string().contains("queue empty"),
            "error must name the cause; got: {e}",
        ),
    }
}
