// main.zig - Neex WASM Engine
// High-performance compute functions for the Neex build system
// Exports: Memory management, XXHash3, Topological Sort

const std = @import("std");

// =============================================================================
// Memory Allocator for WASM
// =============================================================================

// Use page allocator for WASM (grows memory as needed)
var allocator = std.heap.page_allocator;

/// Allocate memory from WASM linear memory
/// Returns pointer offset in WASM memory
export fn alloc(len: usize) ?[*]u8 {
    const slice = allocator.alloc(u8, len) catch return null;
    return slice.ptr;
}

/// Free previously allocated memory
export fn free(ptr: [*]u8, len: usize) void {
    allocator.free(ptr[0..len]);
}

// =============================================================================
// XXHash3 - Ultra-fast Hashing (64-bit)
// =============================================================================

/// Hash a byte array and return the result
/// Input: pointer to data, length of data
/// Output: 64-bit hash split into two 32-bit parts (for JS compatibility)
export fn hash_bytes(ptr: [*]const u8, len: usize) u64 {
    const data = ptr[0..len];
    return std.hash.XxHash3.hash(0, data);
}

/// Get the lower 32 bits of a 64-bit hash
export fn hash_lo(hash: u64) u32 {
    return @truncate(hash);
}

/// Get the upper 32 bits of a 64-bit hash
export fn hash_hi(hash: u64) u32 {
    return @truncate(hash >> 32);
}

/// Hash multiple chunks and combine them
/// Format: [len1: u32][data1: bytes][len2: u32][data2: bytes]...
/// This batches multiple hash operations in one WASM call
export fn hash_batch(ptr: [*]const u8, total_len: usize) u64 {
    var hasher = std.hash.XxHash3.init(0);
    var offset: usize = 0;

    while (offset + 4 <= total_len) {
        // Read chunk length (little-endian u32)
        const chunk_len = std.mem.readInt(u32, ptr[offset..][0..4], .little);
        offset += 4;

        if (offset + chunk_len > total_len) break;

        // Hash this chunk
        hasher.update(ptr[offset..][0..chunk_len]);
        offset += chunk_len;
    }

    return hasher.final();
}

// =============================================================================
// Topological Sort (Kahn's Algorithm)
// =============================================================================

const MAX_NODES = 1024;
const MAX_EDGES = 4096;

var topo_in_degree: [MAX_NODES]u32 = undefined;
var topo_adj: [MAX_EDGES]u32 = undefined;
var topo_adj_start: [MAX_NODES + 1]u32 = undefined;
var topo_result: [MAX_NODES]u32 = undefined;
var topo_queue: [MAX_NODES]u32 = undefined;

/// Initialize topological sort with node count
export fn topo_init(node_count: u32) void {
    for (0..node_count) |i| {
        topo_in_degree[i] = 0;
        topo_adj_start[i] = 0;
    }
    topo_adj_start[node_count] = 0;
}

/// Add an edge from 'from' to 'to'
/// Returns true if successful, false if edge limit reached
export fn topo_add_edge(_from: u32, to: u32, edge_index: u32) bool {
    _ = _from; // Unused in current implementation, stored via topo_set_adj
    if (edge_index >= MAX_EDGES) return false;
    if (to >= MAX_NODES) return false;

    topo_adj[edge_index] = to;
    topo_in_degree[to] += 1;
    return true;
}

/// Set adjacency list boundaries for a node
export fn topo_set_adj(node: u32, start: u32, end: u32) void {
    if (node >= MAX_NODES) return;
    topo_adj_start[node] = start;
    topo_adj_start[node + 1] = end;
}

/// Perform topological sort
/// Returns: number of sorted nodes (< node_count means cycle detected)
export fn topo_sort(node_count: u32) u32 {
    if (node_count > MAX_NODES) return 0;

    var queue_front: u32 = 0;
    var queue_back: u32 = 0;
    var result_count: u32 = 0;

    // Find nodes with in-degree 0
    for (0..node_count) |i| {
        if (topo_in_degree[i] == 0) {
            topo_queue[queue_back] = @intCast(i);
            queue_back += 1;
        }
    }

    // Process queue
    while (queue_front < queue_back) {
        const node = topo_queue[queue_front];
        queue_front += 1;

        topo_result[result_count] = node;
        result_count += 1;

        // Decrease in-degree of neighbors
        const adj_start = topo_adj_start[node];
        const adj_end = topo_adj_start[node + 1];

        for (adj_start..adj_end) |edge_idx| {
            const neighbor = topo_adj[edge_idx];
            topo_in_degree[neighbor] -= 1;

            if (topo_in_degree[neighbor] == 0) {
                topo_queue[queue_back] = neighbor;
                queue_back += 1;
            }
        }
    }

    return result_count;
}

/// Get result at index after topo_sort
export fn topo_get_result(index: u32) u32 {
    if (index >= MAX_NODES) return 0;
    return topo_result[index];
}

// =============================================================================
// String Utilities
// =============================================================================

/// Compare two strings for equality
export fn str_eq(a_ptr: [*]const u8, a_len: usize, b_ptr: [*]const u8, b_len: usize) bool {
    if (a_len != b_len) return false;
    return std.mem.eql(u8, a_ptr[0..a_len], b_ptr[0..b_len]);
}

/// Find substring in string (returns offset or max u32 if not found)
export fn str_find(haystack_ptr: [*]const u8, haystack_len: usize, needle_ptr: [*]const u8, needle_len: usize) u32 {
    const haystack = haystack_ptr[0..haystack_len];
    const needle = needle_ptr[0..needle_len];

    if (std.mem.indexOf(u8, haystack, needle)) |index| {
        return @intCast(index);
    }
    return std.math.maxInt(u32);
}

// =============================================================================
// Version Info
// =============================================================================

export fn get_version() u32 {
    return 0x00010000; // v1.0.0
}

// =============================================================================
// Tests
// =============================================================================

test "hash_bytes produces consistent results" {
    const data = "hello world";
    const hash1 = hash_bytes(data.ptr, data.len);
    const hash2 = hash_bytes(data.ptr, data.len);
    try std.testing.expectEqual(hash1, hash2);
    try std.testing.expect(hash1 != 0);
}

test "alloc and free work correctly" {
    const ptr = alloc(1024) orelse return error.OutOfMemory;
    ptr[0] = 42;
    try std.testing.expectEqual(ptr[0], 42);
    free(ptr, 1024);
}

test "str_eq works" {
    const a = "hello";
    const b = "hello";
    const c = "world";
    try std.testing.expect(str_eq(a.ptr, a.len, b.ptr, b.len));
    try std.testing.expect(!str_eq(a.ptr, a.len, c.ptr, c.len));
}
