const loader = require('./dist/src/loader.js');
console.log('Loader loaded');
try {
  console.log('Library Path:', loader.getLibraryPath());
} catch (e) {
  console.log('Library path error (expected if not built):', e.message);
}
