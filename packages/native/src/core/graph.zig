const std = @import("std");
const mem = std.mem;
const Allocator = mem.Allocator;
const ArrayListUnmanaged = std.ArrayListUnmanaged;
const StringHashMap = std.StringHashMap;

/// Dependency graph node
pub const GraphNode = struct {
    name: []const u8,
    dependencies: [][]const u8,
    dependents: [][]const u8,
};

/// Topological sort result
pub const TopoResult = struct {
    order: [][]const u8,
    has_cycle: bool,
    cycle_nodes: [][]const u8,

    pub fn deinit(self: *TopoResult, allocator: Allocator) void {
        for (self.order) |n| allocator.free(n);
        allocator.free(self.order);
        for (self.cycle_nodes) |n| allocator.free(n);
        allocator.free(self.cycle_nodes);
    }
};

/// Dependency graph builder
pub const DependencyGraph = struct {
    allocator: Allocator,
    nodes: StringHashMap(GraphNode),

    const Self = @This();

    pub fn init(allocator: Allocator) Self {
        return Self{
            .allocator = allocator,
            .nodes = StringHashMap(GraphNode).init(allocator),
        };
    }

    pub fn deinit(self: *Self) void {
        var iter = self.nodes.iterator();
        while (iter.next()) |entry| {
            self.allocator.free(entry.key_ptr.*);
            for (entry.value_ptr.dependencies) |dep| {
                self.allocator.free(dep);
            }
            self.allocator.free(entry.value_ptr.dependencies);
            for (entry.value_ptr.dependents) |dep| {
                self.allocator.free(dep);
            }
            self.allocator.free(entry.value_ptr.dependents);
            self.allocator.free(entry.value_ptr.name);
        }
        self.nodes.deinit();
    }

    /// Add node to graph
    pub fn addNode(self: *Self, name: []const u8, dependencies: [][]const u8) !void {
        const name_copy = try self.allocator.dupe(u8, name);

        var deps_copy = try self.allocator.alloc([]const u8, dependencies.len);
        for (dependencies, 0..) |dep, i| {
            deps_copy[i] = try self.allocator.dupe(u8, dep);
        }

        const node = GraphNode{
            .name = name_copy,
            .dependencies = deps_copy,
            .dependents = &[_][]const u8{},
        };

        try self.nodes.put(name_copy, node);
    }

    /// Build reverse edges (dependents)
    pub fn buildDependents(self: *Self) !void {
        var dependents_map = StringHashMap(ArrayListUnmanaged([]const u8)).init(self.allocator);
        defer {
            var iter = dependents_map.iterator();
            while (iter.next()) |entry| {
                entry.value_ptr.deinit(self.allocator);
            }
            dependents_map.deinit();
        }

        var iter = self.nodes.iterator();
        while (iter.next()) |entry| {
            const node_name = entry.key_ptr.*;

            for (entry.value_ptr.dependencies) |dep| {
                const result = try dependents_map.getOrPut(dep);
                if (!result.found_existing) {
                    result.value_ptr.* = ArrayListUnmanaged([]const u8){};
                }
                try result.value_ptr.append(self.allocator, node_name);
            }
        }

        iter = self.nodes.iterator();
        while (iter.next()) |entry| {
            if (dependents_map.get(entry.key_ptr.*)) |*list| {
                entry.value_ptr.dependents = try list.toOwnedSlice(self.allocator);
            }
        }
    }

    /// Topological sort with cycle detection (Kahn's algorithm)
    pub fn topologicalSort(self: *Self) !TopoResult {
        var in_degree = StringHashMap(usize).init(self.allocator);
        defer in_degree.deinit();

        var iter = self.nodes.iterator();
        while (iter.next()) |entry| {
            try in_degree.put(entry.key_ptr.*, entry.value_ptr.dependencies.len);
        }

        var queue = ArrayListUnmanaged([]const u8){};
        defer queue.deinit(self.allocator);

        iter = in_degree.iterator();
        while (iter.next()) |entry| {
            if (entry.value_ptr.* == 0) {
                try queue.append(self.allocator, entry.key_ptr.*);
            }
        }

        var result = ArrayListUnmanaged([]const u8){};

        while (queue.items.len > 0) {
            const node = queue.orderedRemove(0);
            try result.append(self.allocator, try self.allocator.dupe(u8, node));

            if (self.nodes.get(node)) |graph_node| {
                for (graph_node.dependents) |dependent| {
                    if (in_degree.getPtr(dependent)) |degree| {
                        degree.* -= 1;
                        if (degree.* == 0) {
                            try queue.append(self.allocator, dependent);
                        }
                    }
                }
            }
        }

        const has_cycle = result.items.len != self.nodes.count();
        var cycle_nodes = ArrayListUnmanaged([]const u8){};

        if (has_cycle) {
            iter = in_degree.iterator();
            while (iter.next()) |entry| {
                if (entry.value_ptr.* > 0) {
                    try cycle_nodes.append(self.allocator, try self.allocator.dupe(u8, entry.key_ptr.*));
                }
            }
        }

        return TopoResult{
            .order = try result.toOwnedSlice(self.allocator),
            .has_cycle = has_cycle,
            .cycle_nodes = try cycle_nodes.toOwnedSlice(self.allocator),
        };
    }

    /// Get all dependents of a node (transitive)
    pub fn getTransitiveDependents(self: *Self, node_name: []const u8) ![][]const u8 {
        var visited = StringHashMap(void).init(self.allocator);
        defer visited.deinit();

        var result = ArrayListUnmanaged([]const u8){};

        try self.dfsVisit(node_name, &visited, &result);

        return result.toOwnedSlice(self.allocator);
    }

    fn dfsVisit(
        self: *Self,
        node_name: []const u8,
        visited: *StringHashMap(void),
        result: *ArrayListUnmanaged([]const u8),
    ) !void {
        if (visited.contains(node_name)) return;

        try visited.put(node_name, {});

        if (self.nodes.get(node_name)) |node| {
            for (node.dependents) |dependent| {
                try result.append(self.allocator, try self.allocator.dupe(u8, dependent));
                try self.dfsVisit(dependent, visited, result);
            }
        }
    }

    /// Get all packages affected by changes (smart incremental builds)
    pub fn getAffectedPackages(self: *Self, changed_packages: []const []const u8) ![][]const u8 {
        var affected = StringHashMap(void).init(self.allocator);
        defer affected.deinit();

        var result = ArrayListUnmanaged([]const u8){};

        for (changed_packages) |changed| {
            if (!affected.contains(changed)) {
                try affected.put(changed, {});
                try result.append(self.allocator, try self.allocator.dupe(u8, changed));
            }

            const dependents = try self.getTransitiveDependents(changed);
            defer {
                for (dependents) |d| self.allocator.free(d);
                self.allocator.free(dependents);
            }

            for (dependents) |dependent| {
                if (!affected.contains(dependent)) {
                    try affected.put(dependent, {});
                    try result.append(self.allocator, try self.allocator.dupe(u8, dependent));
                }
            }
        }

        return result.toOwnedSlice(self.allocator);
    }

    /// Export graph as JSON for FFI
    pub fn toJson(self: *Self) ![]const u8 {
        var buffer = ArrayListUnmanaged(u8){};

        try buffer.appendSlice(self.allocator, "{\"nodes\":[");

        var iter = self.nodes.iterator();
        var first = true;
        while (iter.next()) |entry| {
            if (!first) try buffer.append(self.allocator, ',');
            first = false;

            try buffer.appendSlice(self.allocator, "{\"name\":\"");
            try buffer.appendSlice(self.allocator, entry.value_ptr.name);
            try buffer.appendSlice(self.allocator, "\",\"deps\":[");

            for (entry.value_ptr.dependencies, 0..) |dep, i| {
                if (i > 0) try buffer.append(self.allocator, ',');
                try buffer.append(self.allocator, '"');
                try buffer.appendSlice(self.allocator, dep);
                try buffer.append(self.allocator, '"');
            }

            try buffer.appendSlice(self.allocator, "],\"dependents\":[");

            for (entry.value_ptr.dependents, 0..) |dep, i| {
                if (i > 0) try buffer.append(self.allocator, ',');
                try buffer.append(self.allocator, '"');
                try buffer.appendSlice(self.allocator, dep);
                try buffer.append(self.allocator, '"');
            }

            try buffer.appendSlice(self.allocator, "]}");
        }

        try buffer.appendSlice(self.allocator, "]}");
        try buffer.append(self.allocator, 0); // null terminator

        return buffer.toOwnedSlice(self.allocator);
    }
};
