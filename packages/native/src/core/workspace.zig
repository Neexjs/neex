const std = @import("std");
const fs = std.fs;
const mem = std.mem;
const Allocator = mem.Allocator;
const ArrayListUnmanaged = std.ArrayListUnmanaged;
const Thread = std.Thread;

/// Package information
pub const Package = struct {
    name: []const u8,
    path: []const u8,
    version: []const u8,
    dependencies: [][]const u8,

    pub fn deinit(self: *Package, allocator: Allocator) void {
        allocator.free(self.name);
        allocator.free(self.path);
        allocator.free(self.version);
        for (self.dependencies) |dep| {
            allocator.free(dep);
        }
        allocator.free(self.dependencies);
    }
};

/// Workspace scan result (batch result)
pub const WorkspaceResult = struct {
    packages: []Package,
    total_files: usize,
    scan_time_ms: u64,

    pub fn deinit(self: *WorkspaceResult, allocator: Allocator) void {
        for (self.packages) |*pkg| {
            pkg.deinit(allocator);
        }
        allocator.free(self.packages);
    }
};

/// Thread-safe workspace scanner
pub const WorkspaceScanner = struct {
    allocator: Allocator,
    root_path: []const u8,
    ignore_patterns: [][]const u8,
    mutex: Thread.Mutex,
    packages: ArrayListUnmanaged(Package),

    const Self = @This();

    pub fn init(allocator: Allocator, root_path: []const u8) !Self {
        return Self{
            .allocator = allocator,
            .root_path = root_path,
            .ignore_patterns = &[_][]const u8{
                "node_modules",
                ".git",
                "dist",
                "build",
                ".next",
                ".turbo",
                "coverage",
            },
            .mutex = Thread.Mutex{},
            .packages = ArrayListUnmanaged(Package){},
        };
    }

    pub fn deinit(self: *Self) void {
        for (self.packages.items) |*pkg| {
            pkg.deinit(self.allocator);
        }
        self.packages.deinit(self.allocator);
    }

    /// Scan workspace in parallel (BATCH operation)
    pub fn scanParallel(self: *Self) !WorkspaceResult {
        const start = std.time.milliTimestamp();

        // Get CPU count for optimal threading
        const cpu_count = try Thread.getCpuCount();
        const thread_count = @min(cpu_count, 8); // Max 8 threads

        var threads = try self.allocator.alloc(Thread, thread_count);
        defer self.allocator.free(threads);

        // Divide work among threads
        var dirs = ArrayListUnmanaged([]const u8){};
        defer dirs.deinit(self.allocator);

        try self.collectTopLevelDirs(&dirs);

        // Spawn threads
        var i: usize = 0;
        while (i < thread_count and i < dirs.items.len) : (i += 1) {
            threads[i] = try Thread.spawn(.{}, scanWorker, .{ self, dirs.items[i] });
        }

        // Wait for completion
        i = 0;
        while (i < thread_count and i < dirs.items.len) : (i += 1) {
            threads[i].join();
        }

        const end = std.time.milliTimestamp();

        return WorkspaceResult{
            .packages = try self.packages.toOwnedSlice(),
            .total_files = 0,
            .scan_time_ms = @intCast(end - start),
        };
    }

    fn collectTopLevelDirs(self: *Self, dirs: *ArrayListUnmanaged([]const u8)) !void {
        var dir = try fs.cwd().openDir(self.root_path, .{ .iterate = true });
        defer dir.close();

        var iter = dir.iterate();
        while (try iter.next()) |entry| {
            if (entry.kind == .directory) {
                if (!self.shouldIgnore(entry.name)) {
                    const path = try self.allocator.dupe(u8, entry.name);
                    try dirs.append(self.allocator, path);
                }
            }
        }
    }

    fn scanWorker(self: *Self, dir_name: []const u8) void {
        self.scanDirectory(dir_name) catch |err| {
            std.debug.print("Error scanning {s}: {}\n", .{ dir_name, err });
        };
    }

    fn scanDirectory(self: *Self, rel_path: []const u8) !void {
        const full_path = try fs.path.join(self.allocator, &[_][]const u8{ self.root_path, rel_path });
        defer self.allocator.free(full_path);

        var dir = fs.cwd().openDir(full_path, .{ .iterate = true }) catch return;
        defer dir.close();

        const pkg_json_path = try fs.path.join(self.allocator, &[_][]const u8{ full_path, "package.json" });
        defer self.allocator.free(pkg_json_path);

        if (self.fileExists(pkg_json_path)) {
            if (self.parsePackageJson(pkg_json_path, full_path)) |pkg| {
                self.mutex.lock();
                defer self.mutex.unlock();
                self.packages.append(self.allocator, pkg) catch {};
            } else |_| {}
        }

        var iter = dir.iterate();
        while (iter.next() catch null) |entry| {
            if (entry.kind == .directory and !self.shouldIgnore(entry.name)) {
                const sub_path = try fs.path.join(self.allocator, &[_][]const u8{ rel_path, entry.name });
                defer self.allocator.free(sub_path);
                try self.scanDirectory(sub_path);
            }
        }
    }

    fn parsePackageJson(self: *Self, path: []const u8, pkg_path: []const u8) !Package {
        const file = try fs.cwd().openFile(path, .{});
        defer file.close();

        const content = try file.readToEndAlloc(self.allocator, 1024 * 1024);
        defer self.allocator.free(content);

        const name = try self.extractJsonField(content, "name");
        const version = try self.extractJsonField(content, "version");

        return Package{
            .name = name,
            .path = try self.allocator.dupe(u8, pkg_path),
            .version = version,
            .dependencies = &[_][]const u8{},
        };
    }

    fn extractJsonField(self: *Self, json: []const u8, field: []const u8) ![]const u8 {
        const pattern = try std.fmt.allocPrint(self.allocator, "\"{s}\":", .{field});
        defer self.allocator.free(pattern);

        if (mem.indexOf(u8, json, pattern)) |start| {
            const value_start = start + pattern.len;
            if (mem.indexOfScalarPos(u8, json, value_start, '"')) |quote_start| {
                if (mem.indexOfScalarPos(u8, json, quote_start + 1, '"')) |quote_end| {
                    return self.allocator.dupe(u8, json[quote_start + 1 .. quote_end]);
                }
            }
        }
        return error.FieldNotFound;
    }

    fn shouldIgnore(self: *Self, name: []const u8) bool {
        for (self.ignore_patterns) |pattern| {
            if (mem.eql(u8, name, pattern)) return true;
        }
        return false;
    }

    fn fileExists(self: *Self, path: []const u8) bool {
        _ = self;
        fs.cwd().access(path, .{}) catch return false;
        return true;
    }
};
