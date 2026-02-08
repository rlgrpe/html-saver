use std::sync::Arc;
use std::time::Duration;

use html_saver::{
    FsStorage, HtmlSaverBuilder, HtmlSaverError, RegexSanitizer, Saveable, SelectorAction,
    SelectorSanitizer, Storage, SubstringSanitizer,
};
use tempfile::TempDir;
use tokio::sync::Mutex as TokioMutex;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A realistic Saveable implementation simulating a web-scraping response.
/// Generates paths like: `{client_id}/{date}/{time}_{status}_{action}.html`
struct ScrapingResult {
    client_id: String,
    date: String,
    time: String,
    status_code: u16,
    action: String,
    html: String,
}

impl Saveable for ScrapingResult {
    fn content(&self) -> &str {
        &self.html
    }

    fn name(&self) -> String {
        format!(
            "{}/{}/{}_{}_{}.html",
            self.client_id, self.date, self.time, self.status_code, self.action,
        )
    }
}

/// Minimal Saveable for simple tests.
struct SimpleDoc {
    name: String,
    html: String,
}

impl Saveable for SimpleDoc {
    fn content(&self) -> &str {
        &self.html
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}

/// In-memory storage for testing without touching the filesystem.
#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct MemoryStorage {
    files: Arc<TokioMutex<Vec<(String, Vec<u8>)>>>,
}

impl MemoryStorage {
    fn new() -> Self {
        Self {
            files: Arc::new(TokioMutex::new(Vec::new())),
        }
    }
}

impl Storage for MemoryStorage {
    async fn put(&self, key: &str, content: &[u8], _content_type: &str) -> html_saver::Result<()> {
        self.files
            .lock()
            .await
            .push((key.to_string(), content.to_vec()));
        Ok(())
    }
}

/// Storage that always fails -- for testing error paths.
#[derive(Clone)]
struct FailingStorage;

impl Storage for FailingStorage {
    async fn put(
        &self,
        _key: &str,
        _content: &[u8],
        _content_type: &str,
    ) -> html_saver::Result<()> {
        Err(HtmlSaverError::StorageUpload("simulated failure".into()))
    }
}

// ---------------------------------------------------------------------------
// Saveable trait tests
// ---------------------------------------------------------------------------

#[test]
fn saveable_scraping_result_name_format() {
    let req = ScrapingResult {
        client_id: "client-42".into(),
        date: "2024-01-15".into(),
        time: "12-30-00".into(),
        status_code: 200,
        action: "search".into(),
        html: "<html></html>".into(),
    };
    assert_eq!(req.name(), "client-42/2024-01-15/12-30-00_200_search.html");
}

#[test]
fn saveable_scraping_result_content() {
    let req = ScrapingResult {
        client_id: "c".into(),
        date: "d".into(),
        time: "t".into(),
        status_code: 200,
        action: "a".into(),
        html: "<h1>Hello World</h1>".into(),
    };
    assert_eq!(req.content(), "<h1>Hello World</h1>");
}

#[test]
fn saveable_simple_doc() {
    let doc = SimpleDoc {
        name: "page.html".into(),
        html: "<p>test</p>".into(),
    };
    assert_eq!(doc.name(), "page.html");
    assert_eq!(doc.content(), "<p>test</p>");
}

// ---------------------------------------------------------------------------
// FsStorage tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fs_storage_write_and_read() {
    let tmp = TempDir::new().unwrap();
    let storage = FsStorage::new(tmp.path());

    let content = b"<html><body>Test page</body></html>";
    storage
        .put("test.html", content, "text/html")
        .await
        .unwrap();

    let path = tmp.path().join("test.html");
    assert!(path.exists());
    let read = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(read, "<html><body>Test page</body></html>");
}

#[tokio::test]
async fn fs_storage_nested_paths() {
    let tmp = TempDir::new().unwrap();
    let storage = FsStorage::new(tmp.path());

    storage
        .put(
            "2024-01-15/12-30-00_200_abc.html",
            b"<p>nested</p>",
            "text/html",
        )
        .await
        .unwrap();

    let path = tmp.path().join("2024-01-15/12-30-00_200_abc.html");
    assert!(path.exists());
    let read = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(read, "<p>nested</p>");
}

#[tokio::test]
async fn fs_storage_deeply_nested_paths() {
    let tmp = TempDir::new().unwrap();
    let storage = FsStorage::new(tmp.path());

    storage
        .put(
            "clients/42/2024/01/15/result.html",
            b"<div>deep</div>",
            "text/html",
        )
        .await
        .unwrap();

    let path = tmp.path().join("clients/42/2024/01/15/result.html");
    assert!(path.exists());
}

#[tokio::test]
async fn fs_storage_concurrent_writes() {
    let tmp = TempDir::new().unwrap();

    let mut handles = vec![];
    for i in 0..10 {
        let s = FsStorage::new(tmp.path());
        handles.push(tokio::spawn(async move {
            s.put(
                &format!("file_{i}.html"),
                format!("<p>{i}</p>").as_bytes(),
                "text/html",
            )
            .await
            .unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    for i in 0..10 {
        let path = tmp.path().join(format!("file_{i}.html"));
        assert!(path.exists());
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, format!("<p>{i}</p>"));
    }
}

// ---------------------------------------------------------------------------
// End-to-end: HtmlSaver with FsStorage
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fs_save_multiple_documents() {
    let tmp = TempDir::new().unwrap();
    let storage = FsStorage::new(tmp.path());

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(5)
        .flush_interval(Duration::from_millis(50))
        .build::<SimpleDoc>();

    for i in 0..5 {
        handle
            .save(SimpleDoc {
                name: format!("doc_{i}.html"),
                html: format!("<h1>Document {i}</h1>"),
            })
            .unwrap();
    }

    // batch_size=5, so after sending 5 items it should flush
    tokio::time::sleep(Duration::from_millis(200)).await;

    for i in 0..5 {
        let path = tmp.path().join(format!("doc_{i}.html"));
        assert!(path.exists(), "doc_{i}.html should exist");
        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, format!("<h1>Document {i}</h1>"));
    }

    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_batch_flush_by_size() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(2)
        .flush_interval(Duration::from_secs(60)) // Very long, so only size triggers flush
        .build::<SimpleDoc>();

    handle
        .save(SimpleDoc {
            name: "a.html".into(),
            html: "<p>a</p>".into(),
        })
        .unwrap();

    // Not yet flushed (batch_size=2, only 1 sent)
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(files.lock().await.len(), 0);

    handle
        .save(SimpleDoc {
            name: "b.html".into(),
            html: "<p>b</p>".into(),
        })
        .unwrap();

    // Now batch_size reached, should flush
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(files.lock().await.len(), 2);

    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_batch_flush_by_interval() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(100) // Very large, so only interval triggers flush
        .flush_interval(Duration::from_millis(80))
        .build::<SimpleDoc>();

    handle
        .save(SimpleDoc {
            name: "interval.html".into(),
            html: "<p>interval</p>".into(),
        })
        .unwrap();

    // Wait for interval to trigger
    tokio::time::sleep(Duration::from_millis(300)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, "interval.html");

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_sanitizer_pipeline_applied() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .add_sanitizer(SelectorSanitizer::new(vec![(
            "script",
            SelectorAction::RemoveElement,
        )]))
        .add_sanitizer(SubstringSanitizer::new(vec![(
            "SECRET_TOKEN",
            "[REDACTED]",
        )]))
        .add_sanitizer(RegexSanitizer::new(vec![(
            r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",
            "[EMAIL]",
        )]))
        .build::<SimpleDoc>();

    let html = concat!(
        r#"<html><head><script>track("SECRET_TOKEN")</script></head>"#,
        r#"<body><p>Contact user@example.com about SECRET_TOKEN</p></body></html>"#,
    );

    handle
        .save(SimpleDoc {
            name: "sanitized.html".into(),
            html: html.into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    let content = String::from_utf8_lossy(&stored[0].1);
    assert!(!content.contains("<script"));
    assert!(!content.contains("SECRET_TOKEN"));
    assert!(!content.contains("user@example.com"));
    assert!(content.contains("[REDACTED]"));
    assert!(content.contains("[EMAIL]"));

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_prefix_prepended() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .prefix("scrapes/production")
        .build::<SimpleDoc>();

    handle
        .save(SimpleDoc {
            name: "page.html".into(),
            html: "<p>test</p>".into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, "scrapes/production/page.html");

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_prefix_with_saveable_nested_name() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .prefix("html_dumps")
        .build::<ScrapingResult>();

    handle
        .save(ScrapingResult {
            client_id: "acme".into(),
            date: "2024-03-20".into(),
            time: "09-15-00".into(),
            status_code: 200,
            action: "product_list".into(),
            html: "<table><tr><td>Item</td></tr></table>".into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(
        stored[0].0,
        "html_dumps/acme/2024-03-20/09-15-00_200_product_list.html"
    );

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_graceful_shutdown_drains_remaining() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(100) // Large batch, won't flush by size
        .flush_interval(Duration::from_secs(60)) // Long interval, won't flush by time
        .build::<SimpleDoc>();

    for i in 0..5 {
        handle
            .save(SimpleDoc {
                name: format!("drain_{i}.html"),
                html: format!("<p>drain {i}</p>"),
            })
            .unwrap();
    }

    // Items are in buffer, not yet flushed
    // Shutdown should drain them
    handle.shutdown().await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 5, "all items should be drained on shutdown");
}

#[tokio::test]
async fn e2e_sender_clone_from_multiple_tasks() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(100)
        .flush_interval(Duration::from_millis(50))
        .build::<SimpleDoc>();

    let sender1 = handle.sender();
    let sender2 = handle.sender();
    let sender3 = sender1.clone();

    let t1 = tokio::spawn(async move {
        for i in 0..3 {
            sender1
                .save(SimpleDoc {
                    name: format!("task1_{i}.html"),
                    html: format!("<p>task1 {i}</p>"),
                })
                .unwrap();
        }
    });

    let t2 = tokio::spawn(async move {
        for i in 0..3 {
            sender2
                .save(SimpleDoc {
                    name: format!("task2_{i}.html"),
                    html: format!("<p>task2 {i}</p>"),
                })
                .unwrap();
        }
    });

    let t3 = tokio::spawn(async move {
        for i in 0..3 {
            sender3
                .save(SimpleDoc {
                    name: format!("task3_{i}.html"),
                    html: format!("<p>task3 {i}</p>"),
                })
                .unwrap();
        }
    });

    t1.await.unwrap();
    t2.await.unwrap();
    t3.await.unwrap();

    // Wait for interval flush
    tokio::time::sleep(Duration::from_millis(200)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 9, "all 9 items from 3 tasks should be stored");

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_fs_with_prefix_and_scraping_result() {
    let tmp = TempDir::new().unwrap();
    let storage = FsStorage::new(tmp.path());

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .prefix("v1")
        .build::<ScrapingResult>();

    handle
        .save(ScrapingResult {
            client_id: "client-7".into(),
            date: "2024-06-01".into(),
            time: "14-00-00".into(),
            status_code: 200,
            action: "checkout".into(),
            html: "<div>Order confirmed</div>".into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(150)).await;

    let path = tmp
        .path()
        .join("v1/client-7/2024-06-01/14-00-00_200_checkout.html");
    assert!(path.exists());
    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(content, "<div>Order confirmed</div>");

    handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edge_empty_html_content() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .build::<SimpleDoc>();

    handle
        .save(SimpleDoc {
            name: "empty.html".into(),
            html: "".into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].1, b"");

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn edge_special_characters_in_content() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .build::<SimpleDoc>();

    let special_html =
        r#"<p>Specials: &amp; &lt; &gt; "quotes" 'apostrophes' emoji: 🎉 中文 кирилица</p>"#;
    handle
        .save(SimpleDoc {
            name: "special.html".into(),
            html: special_html.into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(String::from_utf8_lossy(&stored[0].1), special_html);

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn edge_long_file_name() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .build::<SimpleDoc>();

    let long_name = format!("{}.html", "a".repeat(200));
    handle
        .save(SimpleDoc {
            name: long_name.clone(),
            html: "<p>long name</p>".into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, long_name);

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn edge_channel_full_small_buffer() {
    let storage = MemoryStorage::new();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(100)
        .flush_interval(Duration::from_secs(60))
        .channel_buffer(2) // Very small buffer
        .build::<SimpleDoc>();

    // Fill the channel
    handle
        .save(SimpleDoc {
            name: "1.html".into(),
            html: "<p>1</p>".into(),
        })
        .unwrap();
    handle
        .save(SimpleDoc {
            name: "2.html".into(),
            html: "<p>2</p>".into(),
        })
        .unwrap();

    // The channel is full now (buffer=2), next send should fail
    let result = handle.save(SimpleDoc {
        name: "3.html".into(),
        html: "<p>3</p>".into(),
    });
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), HtmlSaverError::ChannelClosed));

    handle.shutdown().await;
}

#[tokio::test]
async fn edge_no_prefix_no_sanitizer() {
    let storage = MemoryStorage::new();
    let files = storage.files.clone();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .build::<SimpleDoc>();

    handle
        .save(SimpleDoc {
            name: "bare.html".into(),
            html: "<p>no prefix</p>".into(),
        })
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let stored = files.lock().await;
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, "bare.html");
    assert_eq!(String::from_utf8_lossy(&stored[0].1), "<p>no prefix</p>");

    drop(stored);
    handle.shutdown().await;
}

#[tokio::test]
async fn edge_save_or_log_does_not_panic() {
    let storage = MemoryStorage::new();

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(1)
        .channel_buffer(1)
        .build::<SimpleDoc>();

    // Fill channel
    handle
        .save(SimpleDoc {
            name: "x.html".into(),
            html: "<p>x</p>".into(),
        })
        .unwrap();

    // save_or_log should not panic even when channel is full
    handle.save_or_log(SimpleDoc {
        name: "y.html".into(),
        html: "<p>y</p>".into(),
    });

    // Also test sender's save_or_log
    let sender = handle.sender();
    sender.save_or_log(SimpleDoc {
        name: "z.html".into(),
        html: "<p>z</p>".into(),
    });

    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_failing_storage_does_not_crash_worker() {
    let handle = HtmlSaverBuilder::new(FailingStorage)
        .batch_size(1)
        .build::<SimpleDoc>();

    handle
        .save(SimpleDoc {
            name: "fail.html".into(),
            html: "<p>fail</p>".into(),
        })
        .unwrap();

    // Give worker time to attempt the flush (which will fail internally)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Worker should still be alive; we can send more items
    handle
        .save(SimpleDoc {
            name: "fail2.html".into(),
            html: "<p>fail2</p>".into(),
        })
        .unwrap();

    // Graceful shutdown should still work
    handle.shutdown().await;
}

#[tokio::test]
async fn e2e_large_batch_realistic_scenario() {
    let tmp = TempDir::new().unwrap();
    let storage = FsStorage::new(tmp.path());

    let handle = HtmlSaverBuilder::new(storage)
        .batch_size(25)
        .flush_interval(Duration::from_millis(100))
        .prefix("prod")
        .add_sanitizer(SubstringSanitizer::new(vec![(
            "BEARER_TOKEN_XYZ",
            "[REDACTED]",
        )]))
        .build::<ScrapingResult>();

    // Simulate 50 scraping results from different clients
    for i in 0..50 {
        handle
            .save(ScrapingResult {
                client_id: format!("client-{}", i % 5),
                date: "2024-07-01".into(),
                time: format!("{:02}-00-00", i % 24),
                status_code: if i % 10 == 0 { 500 } else { 200 },
                action: "fetch".into(),
                html: format!(
                    "<html><body><p>Result {i}</p><span>BEARER_TOKEN_XYZ</span></body></html>"
                ),
            })
            .unwrap();
    }

    // Wait for all batches to flush
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle.shutdown().await;

    // Verify a sample of files exist with sanitized content
    let sample_path = tmp
        .path()
        .join("prod/client-0/2024-07-01/00-00-00_500_fetch.html");
    assert!(sample_path.exists());
    let content = tokio::fs::read_to_string(&sample_path).await.unwrap();
    assert!(content.contains("Result 0"));
    assert!(!content.contains("BEARER_TOKEN_XYZ"));
    assert!(content.contains("[REDACTED]"));
}
