# html_saver

A Rust library for background HTML content saving to storage (S3, filesystem) with batching, sanitization, and flexible naming.

## Features

- **Background saving** via a Tokio mpsc channel and a dedicated worker task
- **Batch uploading** by configurable size threshold and time interval
- **HTML sanitization pipeline** with regex, substring, and CSS selector-based sanitizers
- **Trait-based storage backends** -- ships with S3 and filesystem implementations
- **User-defined naming** via the `Saveable` trait
- **Global singleton helper** for convenient access across your application
- **Feature-gated S3 support** -- opt out to avoid pulling in the AWS SDK

## Quick Start

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
html_saver = { git = "https://github.com/rlgrpe/html-saver.git", tag = "v0.1.0" }
```

To disable S3 support and avoid the AWS SDK dependency:

```toml
[dependencies]
html_saver = { git = "https://github.com/rlgrpe/html-saver.git", tag = "v0.1.0", default-features = false }
```

### Full Example

```rust,no_run
use html_saver::{FsStorage, HtmlSaverBuilder, Saveable, SubstringSanitizer};
use std::time::Duration;

// 1. Define a struct implementing Saveable
struct PageSnapshot {
    url: String,
    html: String,
}

impl Saveable for PageSnapshot {
    fn content(&self) -> &str {
        &self.html
    }

    fn name(&self) -> String {
        // Turn "https://example.com/path" into "example.com_path.html"
        let name = self.url.replace("https://", "").replace('/', "_");
        format!("{name}.html")
    }
}

#[tokio::main]
async fn main() {
    // 2. Configure and build the saver
    let storage = FsStorage::new("/tmp/html_dumps");

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(10)
        .flush_interval(Duration::from_secs(5))
        .channel_buffer(500)
        .prefix("snapshots")
        .add_sanitizer(SubstringSanitizer::new(vec![
            ("secret-token", "[REDACTED]"),
        ]))
        .build::<PageSnapshot>();

    // 3. Send items for saving
    handle.save(PageSnapshot {
        url: "https://example.com/page".into(),
        html: "<html><body>secret-token inside</body></html>".into(),
    }).unwrap();

    // 4. Share the sender across tasks
    let sender = handle.sender();
    tokio::spawn(async move {
        sender.save(PageSnapshot {
            url: "https://example.com/other".into(),
            html: "<html><body>Hello</body></html>".into(),
        }).unwrap();
    });

    // 5. Graceful shutdown -- flushes remaining items
    handle.shutdown().await;
}
```

## Storage Backends

### FsStorage

Writes files to a base directory, creating subdirectories as needed.

```rust,no_run
use html_saver::FsStorage;

let storage = FsStorage::new("/var/data/html");
```

### S3Storage

Requires the `s3` feature (enabled by default).

#### From environment credentials

```rust,no_run
use html_saver::S3Storage;

# async fn example() {
let storage = S3Storage::from_env("my-bucket").await;
# }
```

#### From explicit configuration

```rust,no_run
use html_saver::{S3Storage, S3Config, Credentials, Region};

let credentials = Credentials::new(
    "access-key-id",
    "secret-access-key",
    None,  // session token
    None,  // expiry
    "my-app",
);

let config = S3Config::builder()
    .region(Region::new("us-east-1"))
    .endpoint_url("https://s3.example.com")
    .credentials_provider(credentials)
    .force_path_style(true)
    .build();

let storage = S3Storage::from_conf(config, "my-bucket");
```

### Custom Backend

Implement the `Storage` trait to use any backend:

```rust,ignore
use html_saver::{Storage, Result};

struct MyStorage;

impl Storage for MyStorage {
    async fn put(&self, key: &str, content: &[u8], content_type: &str) -> Result<()> {
        // your logic here
        Ok(())
    }
}
```

## Sanitizers

Sanitizers transform HTML content before it is written to storage. They are applied in the order they are added.

### SubstringSanitizer

Simple literal string replacements.

```rust
use html_saver::SubstringSanitizer;
use html_saver::Sanitizer;

let sanitizer = SubstringSanitizer::new(vec![
    ("password123", "***"),
    ("secret-key", "[REDACTED]"),
]);

let result = sanitizer.sanitize("<div>password123</div>");
assert_eq!(result, "<div>***</div>");
```

### RegexSanitizer

Pattern-based replacements using the `regex` crate syntax.

```rust
use html_saver::RegexSanitizer;
use html_saver::Sanitizer;

let sanitizer = RegexSanitizer::new(vec![
    (r"\d{4}-\d{4}-\d{4}-\d{4}", "XXXX-XXXX-XXXX-XXXX"),
    (r"(?i)bearer\s+\S+", "Bearer [REDACTED]"),
]);

let result = sanitizer.sanitize("Card: 1234-5678-9012-3456");
assert_eq!(result, "Card: XXXX-XXXX-XXXX-XXXX");
```

A fallible constructor is also available:

```rust
use html_saver::RegexSanitizer;

let sanitizer = RegexSanitizer::try_new(vec![
    (r"\d+", "N"),
]).expect("invalid regex");
```

### SelectorSanitizer

CSS selector-based transformations using the `scraper` crate. Three actions are available:

| Action | Description |
|--------|-------------|
| `SelectorAction::RemoveElement` | Remove the entire matching element |
| `SelectorAction::RemoveAttr(attr)` | Remove a specific attribute from matching elements |
| `SelectorAction::ReplaceText(text)` | Replace the text content of matching elements |

```rust
use html_saver::{SelectorSanitizer, SelectorAction, Sanitizer};

let sanitizer = SelectorSanitizer::new(vec![
    ("script", SelectorAction::RemoveElement),
    ("a", SelectorAction::RemoveAttr("onclick".into())),
    (".secret", SelectorAction::ReplaceText("[REDACTED]".into())),
]);
```

### Pipeline Composition

Add multiple sanitizers to the builder -- they execute in order:

```rust,no_run
use html_saver::{
    HtmlSaverBuilder, FsStorage, Saveable,
    SubstringSanitizer, RegexSanitizer, SelectorSanitizer, SelectorAction,
};

# struct MyItem { html: String, name: String }
# impl Saveable for MyItem {
#     fn content(&self) -> &str { &self.html }
#     fn name(&self) -> String { self.name.clone() }
# }
let handle = HtmlSaverBuilder::new(FsStorage::new("/tmp/out"))
    .add_sanitizer(SelectorSanitizer::new(vec![
        ("script", SelectorAction::RemoveElement),
    ]))
    .add_sanitizer(SubstringSanitizer::new(vec![
        ("tracking-id-abc", "[REMOVED]"),
    ]))
    .add_sanitizer(RegexSanitizer::new(vec![
        (r"token=[a-f0-9]+", "token=[REDACTED]"),
    ]))
    .build::<MyItem>();
```

You can also build a `SanitizerPipeline` manually:

```rust
use html_saver::{SanitizerPipeline, SubstringSanitizer, RegexSanitizer};

let mut pipeline = SanitizerPipeline::new();
pipeline.add(SubstringSanitizer::new(vec![("secret", "***")]));
pipeline.add(RegexSanitizer::new(vec![(r"\d{4}", "XXXX")]));

let result = pipeline.sanitize("secret code: 1234");
assert_eq!(result, "*** code: XXXX");
```

## Configuration

`HtmlSaverBuilder` exposes the following options:

| Method | Default | Description |
|--------|---------|-------------|
| `batch_size(n)` | `50` | Maximum number of items batched before flushing to storage |
| `flush_interval(duration)` | `5s` | Time interval after which the batch is flushed regardless of size |
| `channel_buffer(n)` | `1000` | Capacity of the mpsc channel between callers and the worker |
| `prefix(str)` | `""` | Prefix prepended to all storage keys (e.g. `"html_dumps"` produces `html_dumps/name.html`) |
| `add_sanitizer(s)` | none | Appends a sanitizer to the pipeline |

## Cargo Features

| Feature | Default | Description |
|---------|---------|-------------|
| `s3` | Yes | Enables the S3 storage backend (`S3Storage`, `S3Config`, `Credentials`, `Region`) via the AWS SDK |
| `rustls-tls` | No | Uses `rustls` as the TLS implementation for the AWS SDK instead of the platform default |

## Global Helper

For applications that need a single shared instance, a global singleton pattern is provided:

```rust,no_run
use html_saver::{init, global, HtmlSaverBuilder, FsStorage, Saveable};

struct MyRequest {
    html: String,
    key: String,
}

impl Saveable for MyRequest {
    fn content(&self) -> &str { &self.html }
    fn name(&self) -> String { self.key.clone() }
}

#[tokio::main]
async fn main() {
    // Initialize once at startup. The returned handle must be kept alive.
    let handle = init::<_, MyRequest>(
        HtmlSaverBuilder::new(FsStorage::new("/tmp/html"))
            .batch_size(20)
    );

    // From anywhere in the application:
    if let Some(sender) = global::<MyRequest>() {
        sender.save(MyRequest {
            html: "<p>Hello</p>".into(),
            key: "page.html".into(),
        }).unwrap();
    }

    handle.shutdown().await;
}
```

**Note:** `init` panics if called more than once. The `global` function returns `None` if `init` has not been called or if the type parameter does not match.

## License

Licensed under either of

- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
