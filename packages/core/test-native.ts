import { Native } from './src/native.js';

console.log("🚀 Testing Neex Native Engine...");

try {
    const start = performance.now();
    const files = Native.scan(process.cwd());
    const end = performance.now();
    
    console.log(`✅ Scanned ${files.length} package.json files in ${(end - start).toFixed(2)}ms`);
    // console.log("Files:", files);
    
    if (files.length > 0) {
        // Hash the first file
        const firstFile = files[0]; 
        console.log(`Hashing ${firstFile}...`);
        const hash = Native.hashFile(firstFile);
        console.log(`Hash: ${hash}`);
    }
    
    console.log("\n🧪 Testing Hashing Strategies on Workspace...");
    
    const startSerial = performance.now();
    const hashSerial = Native.getPackageHash(process.cwd(), 'serial');
    const endSerial = performance.now();
    console.log(`[Serial] Hash: ${hashSerial} (Time: ${(endSerial - startSerial).toFixed(2)}ms)`);

    const startParallel = performance.now();
    const hashParallel = Native.getPackageHash(process.cwd(), 'parallel');
    const endParallel = performance.now();
    console.log(`[Parallel] Hash: ${hashParallel} (Time: ${(endParallel - startParallel).toFixed(2)}ms)`);

    if (hashSerial === hashParallel) {
        console.log("✅ Determinism Verified: Serial and Parallel hashes match.");
    } else {
        console.error("❌ Determinism Failed: Hashes do not match!");
    }

} catch(e) {
    console.error("❌ Native Test Failed:", e);
}
