---
source: docs.rs (Bevy 0.18.0) + Context7 API
library: Bevy
package: bevy
version: 0.18.0
topic: AssetLoader trait, Reader, read_to_end, extensions
fetched: 2026-02-25T12:00:00Z
official_docs: https://docs.rs/bevy/0.18.0/bevy/asset/trait.AssetLoader.html
---

# AssetLoader Trait — Bevy 0.18.0

## Full Trait Definition

```rust
// bevy::asset::AssetLoader (source: bevy_asset/src/loader.rs)
pub trait AssetLoader:
    TypePath
    + Send
    + Sync
    + 'static
{
    type Asset: Asset;
    type Settings: Settings + Default + Serialize + for<'a> Deserialize<'a>;
    type Error: Into<BevyError>;

    // Required method
    fn load(
        &self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> impl ConditionalSendFuture;

    // Provided method
    fn extensions(&self) -> &[&str] { ... }
}
```

### Key Points

- **Supertraits**: `TypePath + Send + Sync + 'static`
- **Not dyn-compatible** (not object-safe)
- The `load` method returns `impl ConditionalSendFuture` — in practice you write `async fn load(...)` in your impl
- `Error` must be `Into<BevyError>` (NOT `Into<Box<dyn Error>>` — this changed from older versions)

## 1. The `load` Method Signature

```rust
fn load(
    &self,
    reader: &mut dyn Reader,       // <-- dyn Reader, NOT &mut Reader
    settings: &Self::Settings,
    load_context: &mut LoadContext<'_>,
) -> impl ConditionalSendFuture;
```

**In implementations, you write it as `async fn`:**

```rust
impl AssetLoader for MyLoader {
    type Asset = MyAsset;
    type Settings = ();
    type Error = MyError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<MyAsset, MyError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        // deserialize bytes...
        Ok(my_asset)
    }
}
```

The return type `impl ConditionalSendFuture` is satisfied by `async fn` returning `Result<Self::Asset, Self::Error>`.

## 2. The `Reader` Type

**Module path**: `bevy::asset::io::Reader`

**Full definition (Bevy 0.18.0):**

```rust
// bevy::asset::io::Reader
pub trait Reader:
    AsyncRead       // from bevy::tasks::futures_lite::AsyncRead
    + Unpin
    + Send
    + Sync
{
    // Required method
    fn seekable(
        &mut self,
    ) -> Result<&mut dyn SeekableReader, ReaderNotSeekableError>;

    // Provided method
    fn read_to_end<'a>(
        &'a mut self,
        buf: &'a mut Vec<u8>,
    ) -> StackFuture<'a, Result<usize, std::io::Error>, {constant}> { ... }
}
```

### Key Details

- **It is `&mut dyn Reader`** in the `load` signature — a trait object, not a concrete type
- `Reader` extends `AsyncRead + Unpin + Send + Sync`
- `AsyncRead` comes from `bevy::tasks::futures_lite::AsyncRead` (re-exported from `futures-lite`)
- **New in 0.18**: `seekable()` method replaces the old `AsyncSeekForward` approach
- `Reader` is implemented for: `Box<dyn Reader>`, `async_fs::File`, `VecReader`, `SliceReader`, `TransactionLockedReader`

## 3. `read_to_end` on Reader

```rust
fn read_to_end<'a>(
    &'a mut self,
    buf: &'a mut Vec<u8>,
) -> StackFuture<'a, Result<usize, std::io::Error>, {constant}>
```

- **Provided method** on the `Reader` trait (not required to implement)
- Reads the entire contents of the reader and **appends** them to the provided `Vec<u8>`
- Returns the number of bytes read
- Uses `StackFuture` (stack-allocated future) internally
- **Usage**: `reader.read_to_end(&mut bytes).await?;`
- The default implementation calls `poll_read` repeatedly to fill the buffer 32 bytes at a time
- Implementors are encouraged to override this for better performance

**Note**: You can also use `AsyncReadExt::read_to_end` from `futures_lite` since `Reader: AsyncRead`, but the `Reader` trait's own `read_to_end` is preferred as it may be optimized by implementors.

### Typical Usage Pattern

```rust
async fn load(
    &self,
    reader: &mut dyn Reader,
    _settings: &(),
    _load_context: &mut LoadContext<'_>,
) -> Result<MyAsset, MyError> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await?;
    let data: MyData = ron::de::from_bytes(&bytes)?;
    Ok(MyAsset(data))
}
```

## 4. The `extensions()` Method

```rust
fn extensions(&self) -> &[&str]
```

- **Provided method** (has a default implementation returning `&[]`)
- Returns a list of file extensions supported by this loader, **without the preceding dot**
- Example: `&["ron"]` not `&[".ron"]`
- Users of the loader may still load files with non-matching extensions

### Example

```rust
fn extensions(&self) -> &[&str] {
    &["ron"]
}
```

## 5. Changes from Bevy 0.15/0.16 to 0.18

### Error Type: `Into<BevyError>` (was `Into<Box<dyn Error + Send + Sync>>`)

**0.15/0.16:**
```rust
type Error: Into<Box<dyn Error + Send + Sync>>;
```

**0.18:**
```rust
type Error: Into<BevyError>;
```

`BevyError` is Bevy's unified error type. Most error types that implement `std::error::Error + Send + Sync + 'static` will work via the blanket impl.

### Reader: `&mut dyn Reader` with `seekable()` (was `AsyncSeekForward`)

**0.15**: Replaced `AsyncSeek` with `AsyncSeekForward` on `Reader` (forward-only seeking).

**0.18**: Removed `AsyncSeekForward` entirely. `Reader` now has:
- A `seekable()` method that returns `Result<&mut dyn SeekableReader, ReaderNotSeekableError>`
- `SeekableReader` provides full `AsyncSeek` functionality
- Loaders that need seeking should call `reader.seekable()` and handle the fallback case

**Fallback pattern for seekable readers:**
```rust
let mut fallback_reader;
let reader = match reader.seekable() {
    Ok(seek) => seek,
    Err(_) => {
        fallback_reader = VecReader::new(Vec::new());
        reader.read_to_end(&mut fallback_reader.bytes).await.unwrap();
        &mut fallback_reader
    }
};
reader.seek(SeekFrom::Start(10)).await.unwrap();
```

### Return Type: `impl ConditionalSendFuture` (unchanged since 0.15)

The `load` method has used `-> impl ConditionalSendFuture` since 0.15. You write `async fn` in your impl.

### `read_to_end` uses `StackFuture` (was `BoxedFuture` in older versions)

The `read_to_end` method now returns a `StackFuture` (stack-allocated) instead of a `BoxedFuture` (heap-allocated), for better performance.

## Complete RonLoader<T> Example (Bevy 0.18)

```rust
use bevy::prelude::*;
use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use thiserror::Error;

#[derive(Asset, TypePath, Debug)]
pub struct RonAsset<T: Send + Sync + 'static + std::fmt::Debug + TypePath>(pub T);

#[derive(Default)]
pub struct RonLoader<T> {
    _marker: std::marker::PhantomData<T>,
}

#[derive(Debug, Error)]
pub enum RonLoaderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("RON deserialization error: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

// BevyError has a blanket From impl for types implementing Error + Send + Sync + 'static
// so RonLoaderError works as Into<BevyError> automatically.

impl<T> AssetLoader for RonLoader<T>
where
    T: DeserializeOwned + Send + Sync + 'static + std::fmt::Debug + TypePath,
{
    type Asset = RonAsset<T>;
    type Settings = ();
    type Error = RonLoaderError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<RonAsset<T>, RonLoaderError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let value: T = ron::de::from_bytes(&bytes)?;
        Ok(RonAsset(value))
    }

    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
}
```

**Note on dependencies (from Bevy 0.18.0 Cargo.toml):**
- `ron = "^0.12"` (Bevy 0.18 uses ron 0.12)
- `serde = "^1"`
- `thiserror = "^2.0"`
