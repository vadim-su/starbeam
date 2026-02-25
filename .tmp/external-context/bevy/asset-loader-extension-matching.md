---
source: Bevy GitHub source code (main branch + v0.15.3)
library: bevy
package: bevy_asset
topic: AssetLoader extension matching algorithm
fetched: 2026-02-25T12:00:00Z
official_docs: https://docs.rs/bevy/latest/bevy/asset/trait.AssetLoader.html
source_files:
  - https://github.com/bevyengine/bevy/blob/main/crates/bevy_asset/src/server/loaders.rs
  - https://github.com/bevyengine/bevy/blob/main/crates/bevy_asset/src/path.rs
---

# Bevy AssetLoader Extension Matching Algorithm

## How Extensions Are Registered

When you register an `AssetLoader`, Bevy calls `extensions()` which returns `&[&str]`.
Each extension string is stored as an **exact key** in a `HashMap<Box<str>, Vec<usize>>` called `extension_to_loaders`.

```rust
// From loaders.rs - registration
for extension in AssetLoader::extensions(&*loader) {
    let list = self
        .extension_to_loaders
        .entry((*extension).into())  // exact string key
        .or_default();
    list.push(loader_index);
}
```

Extensions are stored **without the leading dot**, as plain strings. For example:
- `"ron"` → matches files ending in `.ron`
- `"tiles.ron"` → matches files ending in `.tiles.ron`
- `"animgraph.ron"` → matches files ending in `.animgraph.ron` (real Bevy example: `AnimationGraphAssetLoader`)

## How Extensions Are Matched Against File Paths

### Step 1: Extract the "full extension" from the file path

`AssetPath::get_full_extension()` extracts **everything after the first `.`** in the filename:

```rust
// From path.rs
pub fn get_full_extension(&self) -> Option<String> {
    let file_name = self.path().file_name()?.to_str()?;
    let index = file_name.find('.')?;       // finds FIRST dot
    let mut extension = file_name[index + 1..].to_owned();

    // Strip off any query parameters
    let query = extension.find('?');
    if let Some(offset) = query {
        extension.truncate(offset);
    }

    Some(extension)
}
```

**Examples:**
| File path | `file_name` | `get_full_extension()` |
|---|---|---|
| `tiles.ron` | `tiles.ron` | `"ron"` |
| `tiles.registry.ron` | `tiles.registry.ron` | `"registry.ron"` |
| `my_asset.config.ron` | `my_asset.config.ron` | `"config.ron"` |
| `data.tiles.registry.ron` | `data.tiles.registry.ron` | `"tiles.registry.ron"` |
| `scene.scn.ron` | `scene.scn.ron` | `"scn.ron"` |

### Step 2: Try the full extension first, then progressively shorter suffixes

The matching algorithm in `get_by_path()` and `find()` works as follows:

```rust
// From loaders.rs - get_by_path
pub(crate) fn get_by_path(&self, path: &AssetPath<'_>) -> Option<MaybeAssetLoader> {
    let extension = path.get_full_extension()?;

    let result = core::iter::once(extension.as_str())
        .chain(AssetPath::iter_secondary_extensions(&extension))
        .filter_map(|extension| self.extension_to_loaders.get(extension)?.last().copied())
        .find_map(|index| self.get_by_index(index))?;

    Some(result)
}
```

`iter_secondary_extensions` strips one dot-segment at a time from the left:

```rust
// From path.rs
pub(crate) fn iter_secondary_extensions(full_extension: &str) -> impl Iterator<Item = &str> {
    full_extension.char_indices().filter_map(|(i, c)| {
        if c == '.' {
            Some(&full_extension[i + 1..])
        } else {
            None
        }
    })
}
```

### The Complete Matching Order (most specific → least specific)

For a file `my_asset.config.ron`, the algorithm tries these extensions **in order**:

1. `"config.ron"` (full extension)
2. `"ron"` (secondary extension — everything after the next `.`)

For a file `data.tiles.registry.ron`:

1. `"tiles.registry.ron"` (full extension)
2. `"registry.ron"` (secondary)
3. `"ron"` (secondary)

**The first match wins.** This means more specific (longer) extensions take priority.

## Answers to Specific Questions

### Q1: If I register extension `"tiles.ron"`, will it match `"tiles.registry.ron"`?

**NO.** Here's why:

For the file `tiles.registry.ron`:
- Full extension = `"registry.ron"`
- Secondary extensions = `["ron"]`

The algorithm tries: `"registry.ron"` → `"ron"`. It **never** tries `"tiles.ron"`.

The extension `"tiles.ron"` would only match files like:
- `something.tiles.ron` (where `tiles.ron` is the full extension)
- `foo.bar.tiles.ron` (where `tiles.ron` appears as a secondary extension)

### Q2: How does Bevy match compound/multi-part extensions like `"foo.bar.ron"`?

Extension matching is **exact string comparison** against a HashMap key. The algorithm:

1. Extracts the full extension (everything after the first `.` in the filename)
2. Tries that exact string against the HashMap
3. Progressively strips the leftmost dot-segment and tries again
4. First match wins

So registering `"foo.bar.ron"` will match:
- `asset.foo.bar.ron` → full ext is `"foo.bar.ron"` → **exact match on first try**
- `prefix.asset.foo.bar.ron` → full ext is `"asset.foo.bar.ron"` → tries `"asset.foo.bar.ron"` (miss) → tries `"foo.bar.ron"` → **match on second try**

It will NOT match:
- `asset.bar.ron` → full ext is `"bar.ron"`, secondaries are `["ron"]` → never tries `"foo.bar.ron"`
- `asset.foo.ron` → full ext is `"foo.ron"`, secondaries are `["ron"]` → never tries `"foo.bar.ron"`

### Q3: What extension to register for `tiles.registry.ron`, `player.def.ron`, `world.config.ron`?

These files have different full extensions:
- `tiles.registry.ron` → full ext: `"registry.ron"`, secondaries: `["ron"]`
- `player.def.ron` → full ext: `"def.ron"`, secondaries: `["ron"]`
- `world.config.ron` → full ext: `"config.ron"`, secondaries: `["ron"]`

**Option A: Register extension `"ron"`** — This is the simplest approach. All three files will fall through to the `"ron"` secondary extension. But this will also match ANY `.ron` file, so you'd conflict with other `.ron` loaders (like Bevy's scene loader which uses `"scn.ron"`).

**Option B: Register multiple specific extensions** — Register all the compound extensions you need:
```rust
fn extensions(&self) -> &[&str] {
    &["registry.ron", "def.ron", "config.ron"]
}
```
This is the most precise approach. Each file matches on its full extension. No conflicts with other `.ron` loaders.

**Option C: Use a single custom compound extension** — Rename your files to use a common compound extension:
- `tiles.myformat.ron` → register `"myformat.ron"`
- `player.myformat.ron`
- `world.myformat.ron`

This is the pattern Bevy itself uses (e.g., `"animgraph.ron"` for animation graphs, `"scn.ron"` for scenes).

## Real-World Bevy Examples of Compound Extensions

From the Bevy source code:
- `AnimationGraphAssetLoader` → `["animgraph.ron", "animgraph"]`
- Scene files → `["scn", "scn.ron"]`

## Key Takeaway

Bevy's extension matching is a **suffix-based fallback chain**, not a substring or glob match. It tries the full extension first, then progressively shorter suffixes by stripping dot-segments from the left. Registration is by **exact string**. The first registered extension that matches in the fallback chain wins.
