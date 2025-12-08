/**
 * High-Performance LRU Cache
 * O(1) get/set operations using Map's built-in ordering
 */

export class LRUCache<K, V> {
    private cache: Map<K, V>;
    private readonly maxSize: number;
    
    // Statistics
    private hits = 0;
    private misses = 0;

    constructor(maxSize: number = 10000) {
        this.cache = new Map();
        this.maxSize = maxSize;
    }

    /**
     * Get value from cache
     * Moves item to end (most recently used)
     */
    get(key: K): V | undefined {
        const value = this.cache.get(key);
        
        if (value === undefined) {
            this.misses++;
            return undefined;
        }
        
        this.hits++;
        
        // Move to end (most recently used)
        this.cache.delete(key);
        this.cache.set(key, value);
        
        return value;
    }

    /**
     * Set value in cache
     * Evicts least recently used if at capacity
     */
    set(key: K, value: V): void {
        // If key exists, delete to update position
        if (this.cache.has(key)) {
            this.cache.delete(key);
        }
        
        // Evict LRU if at capacity
        if (this.cache.size >= this.maxSize) {
            const firstKey = this.cache.keys().next().value;
            if (firstKey !== undefined) {
                this.cache.delete(firstKey);
            }
        }
        
        this.cache.set(key, value);
    }

    /**
     * Check if key exists
     */
    has(key: K): boolean {
        return this.cache.has(key);
    }

    /**
     * Delete key from cache
     */
    delete(key: K): boolean {
        return this.cache.delete(key);
    }

    /**
     * Clear all entries
     */
    clear(): void {
        this.cache.clear();
        this.hits = 0;
        this.misses = 0;
    }

    /**
     * Get current size
     */
    get size(): number {
        return this.cache.size;
    }

    /**
     * Get cache hit rate (0-1)
     */
    get hitRate(): number {
        const total = this.hits + this.misses;
        if (total === 0) return 0;
        return this.hits / total;
    }

    /**
     * Get statistics
     */
    get stats(): { hits: number; misses: number; size: number; hitRate: number } {
        return {
            hits: this.hits,
            misses: this.misses,
            size: this.cache.size,
            hitRate: this.hitRate
        };
    }

    /**
     * Get all keys (for debugging)
     */
    keys(): IterableIterator<K> {
        return this.cache.keys();
    }

    /**
     * Get all values (for debugging)
     */
    values(): IterableIterator<V> {
        return this.cache.values();
    }
}

// Global hash cache instance
export const hashCache = new LRUCache<string, bigint>(10000);

export default LRUCache;
