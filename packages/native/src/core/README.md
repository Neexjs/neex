# Core Engine Modules

## workspace.zig
**Purpose**: Multi-threaded workspace scanner with batch processing

**Features**:
- Parallel directory traversal using thread pool
- Automatic CPU detection for optimal threading
- Batch JSON result (single FFI call)
- Ignore patterns (node_modules, .git, dist, etc.)
- Thread-safe package collection

**Performance**: 10x faster than Node.js fs.readdir

**Usage from TypeScript**:
```typescript
const result = Native.scanWorkspaceBatch(rootPath);
// Returns: { packages: [...], total_files: N, scan_time_ms: X }
```

**Why Batch?**: Avoids FFI overhead by returning all data in one call instead of N calls.
