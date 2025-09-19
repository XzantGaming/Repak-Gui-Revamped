# IoStore Oodle Compression: Revert Guide

This document explains exactly how to revert the Oodle compression changes made to `IoStoreWriter` if needed. Keep this file with the code so you (or anyone else) can quickly undo the change and restore the original behavior.

## Summary of the change (what we added)

- File edited: `retoc-rivals/src/iostore_writer.rs`
- Added import of `compression::{self, CompressionMethod}`.
- Registered Oodle in the TOC method table (so index 1 = Oodle) in `IoStoreWriter::new(...)`.
- Modified `IoStoreWriter::write_chunk(...)` to try Oodle-compress each block and only use it when it shrinks the data; otherwise store uncompressed blocks.

Behavior after the change:
- Index 0 = None (no compression)
- Index 1 = Oodle (compressed)
- Hash remains over the uncompressed data (IoStore convention)
- Very small blocks are left uncompressed.

## How to fully revert (step-by-step)

Perform these three edits in `retoc-rivals/src/iostore_writer.rs`.

1) Remove Oodle import at the top

Delete this line if present:
```rust
use crate::compression::{self, CompressionMethod};
```

2) Remove method-table registration in `IoStoreWriter::new(...)`

Delete the block:
```rust
// Register compression methods for this container.
// Convention: index 0 = None (implicit), index 1 = Oodle.
 toc.compression_methods.push(CompressionMethod::Oodle);
```

After removal, the `toc` setup should end with:
```rust
let mut toc = Toc::new();
toc.compression_block_size = 0x10000;
toc.version = toc_version;
toc.container_id = FIoContainerId::from_name(&name);
toc.directory_index.mount_point = mount_point;
toc.partition_size = u64::MAX;
```

3) Replace the per-block compression loop in `write_chunk(...)` with the original uncompressed version

Find the `for block in data.chunks(...)` loop inside `IoStoreWriter::write_chunk(...)` and replace the entire loop body with this uncompressed variant:
```rust
let mut hasher = blake3::Hasher::new();
for block in data.chunks(self.toc.compression_block_size as usize) {
    self.cas_stream.write_all(block)?;
    hasher.update(block);
    let compressed_size = block.len() as u32;
    let uncompressed_size = block.len() as u32;
    let compression_method_index = 0; // None
    self.toc
        .compression_blocks
        .push(FIoStoreTocCompressedBlockEntry::new(
            offset,
            compressed_size,
            uncompressed_size,
            compression_method_index,
        ));
    offset += compressed_size as u64;
}
```

That’s it. This restores the original "no compression" behavior for IoStore.

## How to re-apply the compression change later

If you need to re-enable Oodle compression again, re-apply the three steps that were reversed above:

1) Add import:
```rust
use crate::compression::{self, CompressionMethod};
```

2) Register Oodle in TOC in `IoStoreWriter::new(...)`:
```rust
toc.compression_methods.push(CompressionMethod::Oodle); // index 1 = Oodle
```

3) Use the compress-when-beneficial loop in `write_chunk(...)`:
```rust
let mut hasher = blake3::Hasher::new();
const MIN_COMPRESS_SIZE: usize = 1024; // avoid compressing tiny blocks
for block in data.chunks(self.toc.compression_block_size as usize) {
    // Hash over uncompressed data (IoStore convention)
    hasher.update(block);

    let try_compress = block.len() >= MIN_COMPRESS_SIZE;
    let mut compressed = Vec::new();
    let compressed_ok = if try_compress {
        compression::compress(CompressionMethod::Oodle, block, &mut compressed).is_ok()
    } else {
        false
    };

    let (bytes_to_write, compression_method_index) = if compressed_ok && compressed.len() < block.len() {
        (&compressed[..], 1u8) // 1 => Oodle
    } else {
        (block, 0u8) // 0 => None
    };

    self.cas_stream.write_all(bytes_to_write)?;

    let compressed_size = bytes_to_write.len() as u32;
    let uncompressed_size = block.len() as u32;

    self.toc
        .compression_blocks
        .push(FIoStoreTocCompressedBlockEntry::new(
            offset,
            compressed_size,
            uncompressed_size,
            compression_method_index,
        ));
    offset += compressed_size as u64;
}
```

## Optional runtime toggle (if you want)

If you’d like a quick on/off switch without code edits, you can wrap the compression with an environment variable. Example (not applied by default):

- At the top of `write_chunk(...)`:
```rust
let compress_enabled = std::env::var_os("RETOC_IOSTORE_COMPRESS").is_some();
```
- Use `compress_enabled` to guard the compression attempt:
```rust
let try_compress = compress_enabled && block.len() >= MIN_COMPRESS_SIZE;
```
Then you can enable compression by setting `RETOC_IOSTORE_COMPRESS=1` before running, and disable by unsetting it.

## Verification checklist

- Build succeeds:
```
cargo build -p repak-gui --release
```
- For compressed outputs, check `.utoc` contains `compression_methods` with `Oodle` and some blocks have method index `1`.
- Run any existing `verify` or `info/list` actions to ensure hashes and method tables are consistent.

## Notes / pitfalls

- Do not set a non-zero `compression_method_index` unless you wrote compressed bytes and registered a matching method in `toc.compression_methods`.
- IoStore hashes must be computed over the uncompressed data.
- It’s normal to leave small or incompressible blocks uncompressed.

## Runtime toggle (now implemented)

We added a runtime toggle so you can switch IoStore compression ON/OFF without code edits:

- Default behavior: COMPRESSION OFF. Containers are written uncompressed (no Oodle in `compression_methods`, all blocks `compression_method_index = 0`). This matches the configuration that reliably loads in-game for third-party mods.
- Opt-in compression: Set environment variable `RETOC_IOSTORE_COMPRESS` to enable Oodle compression. When enabled, the UTOC registers Oodle and blocks are compressed only when it shrinks data.

### Windows PowerShell example

Enable compression for a single run:

```powershell
$env:RETOC_IOSTORE_COMPRESS = "1"
cargo run -p repak-gui --release
# or run your binary directly after building
```

Disable compression (restore default) in the same session:

```powershell
Remove-Item Env:RETOC_IOSTORE_COMPRESS
```

### Why default OFF?

Some games (e.g., Marvel Rivals) appeared to reject newly generated UTOC/UCAS with declared block compression in third‑party mod containers. Turning compression OFF restores the previously working behavior while still allowing you to explicitly enable it for testing or size-sensitive workflows.

## Quick start

- To produce compatible IOStore mods: run without `RETOC_IOSTORE_COMPRESS` (default). This writes uncompressed blocks and no Oodle entry in the UTOC method table.
- To test compressed UCAS: set `RETOC_IOSTORE_COMPRESS=1` and rebuild. Verify with `retoc list`/`retoc info` and in-game testing.

## Reverting or re-enabling (two options)

1) No-code toggle (recommended)

- Turn compression OFF: ensure `RETOC_IOSTORE_COMPRESS` is NOT set.
- Turn compression ON: set `RETOC_IOSTORE_COMPRESS=1`.

2) Full code revert (hard disable) — see steps above

- Remove the Oodle registration in `IoStoreWriter::new(...)` and restore the uncompressed write loop in `write_chunk(...)` as documented earlier in this file. This permanently disables IoStore compression regardless of environment variables.

## Validation checklist (updated)

- When OFF (default):
  - `compression_methods` is empty (no Oodle)
  - All blocks have `compression_method_index = 0`
  - Game should load these IOStore containers reliably
- When ON:
  - `compression_methods` includes `Oodle`
  - Some blocks may have `compression_method_index = 1`
  - UCAS size should reduce when blocks are compressible
